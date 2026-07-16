use std::collections::HashMap;

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, DeriveInput, Meta, Lit,
    punctuated::Punctuated, Token, Path, Ident,
    Type, PathArguments, GenericArgument,
    Data, Fields,
};
use syn::parse::Parser;
use convert_case::{Case, Casing};
use pluralizer::pluralize;

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap());
    }
    result
}

fn relation_plural_fn_name_from_path(path: &Path, span: proc_macro2::Span) -> Ident {
    let segment = path.segments.last().expect("Expected a valid type path");
    let snake = segment.ident.to_string().to_case(Case::Snake);
    let plural = pluralize(&snake, 2, false);
    Ident::new(&plural, span)
}

/// Returns (surreal_type, extra_nested_type_names)
///
/// Special sentinel values:
///   "__dynamic__"        — type is an unknown struct, resolve via SurrealType trait at runtime
///   "__dynamic_option__" — same but wrapped in Option<>
///   "__record__:<table>" — a RecordId FK whose table is known from relation metadata
fn rust_type_to_surreal(
    field_name: &str,
    ty: &Type,
    table_name: &str,
    field_prefix: &str,
    // Map of field_name -> related table name, built from #[belongs_to] attrs
    fk_map: &HashMap<String, String>,
) -> (String, Vec<String>) {
    match ty {
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last().unwrap();
            let ident = segment.ident.to_string();
            match ident.as_str() {
                "String" => ("string".to_string(), vec![]),
                "bool"   => ("bool".to_string(),   vec![]),
                "i8" | "i16" | "i32" | "i64" |
                "u8" | "u16" | "u32" | "u64" |
                "usize" | "isize" => ("int".to_string(), vec![]),
                "f32" | "f64" => ("float".to_string(), vec![]),

                "RecordId" => {
                    if field_name == "id" {
                        ("record".to_string(), vec![])
                    } else if let Some(related_table) = fk_map.get(field_name) {
                        // FK to a known related table — emit record<table>
                        (format!("record<{}>", related_table), vec![])
                    } else {
                        // Generic FK — keep old behaviour
                        (format!("record<{}>", field_name), vec![])
                    }
                }

                "Option" => {
                    if let PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            let (inner_type, extras) =
                                rust_type_to_surreal(field_name, inner, table_name, field_prefix, fk_map);
                            if inner_type == "__dynamic__" {
                                return ("__dynamic_option__".to_string(), extras);
                            }
                            return (format!("option<{}>", inner_type), extras);
                        }
                    }
                    ("option<string>".to_string(), vec![])
                }

                "Vec" => {
                    if let PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            let (inner_type, extra) =
                                rust_type_to_surreal(field_name, inner, table_name, field_prefix, fk_map);
                            return (format!("array<{}>", inner_type), extra);
                        }
                    }
                    ("array".to_string(), vec![])
                }

                // Unknown type → nested object/enum resolved at runtime via SurrealType trait
                _ => ("__dynamic__".to_string(), vec![ident.clone()]),
            }
        }
        _ => ("any".to_string(), vec![]),
    }
}

// ─────────────────────────────────────────────────────────────
//  ARG PARSERS
// ─────────────────────────────────────────────────────────────

struct BelongsToArgs {
    path: syn::Path,
    fk:   Option<String>,
    to:   Option<String>,
}

impl syn::parse::Parse for BelongsToArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let path = input.parse::<syn::Path>()?;
        let mut fk = None;
        let mut to = None;
        while input.peek(syn::Token![,]) {
            let _: syn::Token![,] = input.parse()?;
            if input.is_empty() { break; }
            let key: syn::Ident = input.parse()?;
            let _: syn::Token![=] = input.parse()?;
            let val: syn::LitStr = input.parse()?;
            match key.to_string().as_str() {
                "fk" => fk = Some(val.value()),
                "to" => to = Some(val.value()),
                other => return Err(syn::Error::new(
                    key.span(),
                    format!("unknown key `{}` — expected `fk` or `to`", other),
                )),
            }
        }
        Ok(BelongsToArgs { path, fk, to })
    }
}

struct HasManyArgs {
    path: syn::Path,
    fk:   Option<String>,
    to:   Option<String>,   // override the pluralized method/registry name
}

impl syn::parse::Parse for HasManyArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let path = input.parse::<syn::Path>()?;
        let mut fk = None;
        let mut to = None;
        while input.peek(syn::Token![,]) {
            let _: syn::Token![,] = input.parse()?;
            if input.is_empty() { break; }
            let key: syn::Ident = input.parse()?;
            let _: syn::Token![=] = input.parse()?;
            let val: syn::LitStr = input.parse()?;
            match key.to_string().as_str() {
                "fk" => fk = Some(val.value()),
                "to" => to = Some(val.value()),
                other => return Err(syn::Error::new(
                    key.span(),
                    format!("unknown key `{}` — expected `fk` or `to`", other),
                )),
            }
        }
        Ok(HasManyArgs { path, fk, to })
    }
}

// ─────────────────────────────────────────────────────────────
//  RELATION ATTRS
// ─────────────────────────────────────────────────────────────

enum RelationAttr {
    BelongsTo {
        path: syn::Path,
        fk:   Option<String>,
        to:   Option<String>,
    },
    HasMany {
        path: syn::Path,
        fk:   Option<String>,
        to:  Option<String>
    },
    BelongsToMany {
        related:  syn::Path,
        pivot:    syn::Path,
        is_left:  bool,
    },
}

fn collect_relation_attrs(attrs: &[syn::Attribute]) -> Vec<RelationAttr> {
    let mut out = vec![];
    for attr in attrs {
        if attr.path().is_ident("belongs_to") {
            let args: BelongsToArgs = attr.parse_args()
                .expect("invalid #[belongs_to(...)] syntax");
            out.push(RelationAttr::BelongsTo { path: args.path, fk: args.fk, to: args.to });
        }
        if attr.path().is_ident("has_many") {
            let args: HasManyArgs = attr.parse_args()
                .expect("invalid #[has_many(...)] syntax");
            out.push(RelationAttr::HasMany { path: args.path, fk: args.fk ,to:args.to});
        }
        if attr.path().is_ident("belongs_to_many") {
            if let Ok(syn::Meta::List(list)) = attr.meta.clone().try_into() {
                let parsed = Punctuated::<Path, Token![,]>::parse_terminated
                    .parse2(list.tokens.clone());
                if let Ok(args) = parsed {
                    let mut iter = args.iter();
                    if let (Some(related), Some(pivot), Some(side)) =
                        (iter.next(), iter.next(), iter.next())
                    {
                        let is_left = side.get_ident()
                            .map(|i| i == "left")
                            .unwrap_or(true);
                        out.push(RelationAttr::BelongsToMany {
                            related: related.clone(),
                            pivot:   pivot.clone(),
                            is_left,
                        });
                    }
                }
            }
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────
//  Build a fk_map from relation attrs + struct fields
//  (field_name -> related_table)
// ─────────────────────────────────────────────────────────────

fn build_fk_map(
    relations: &[RelationAttr],
    fields: &syn::Fields,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for rel in relations {
        if let RelationAttr::BelongsTo { path, fk, .. } = rel {
            let related_table = to_snake_case(&path.segments.last().unwrap().ident.to_string());
            let fk_name = resolve_fk(&related_table, fields, fk.as_ref());
            map.insert(fk_name, related_table);
        }
    }
    map
}

fn resolve_fk(
    relation: &str,
    fields: &syn::Fields,
    custom_fk: Option<&String>,
) -> String {
    if let Some(fk) = custom_fk {
        return fk.clone();
    }
    let candidates = [relation.to_string(), format!("{}_id", relation)];
    for candidate in candidates {
        let exists = fields.iter().any(|f| {
            f.ident.as_ref().map(|i| i == &candidate).unwrap_or(false)
        });
        if exists { return candidate; }
    }
    panic!(
        "No foreign key found for relation `{}`. Expected `{}` or `{}_id`",
        relation, relation, relation
    );
}

// ─────────────────────────────────────────────────────────────
//  Shared helper: emit schema field tokens for a list of fields,
//  returning (static_sql_string, dynamic_token_vec)
//
//  `fk_map` is passed so RecordId fields get record<table> types.
// ─────────────────────────────────────────────────────────────

fn emit_schema_fields(
    fields: &syn::Fields,
    table_lit: &syn::LitStr,
    span: proc_macro2::Span,
    fk_map: &HashMap<String, String>,
    skip_id: bool,
) -> (String, Vec<proc_macro2::TokenStream>) {
    let mut static_sql   = String::new();
    let mut dynamic_toks = vec![];

    for field in fields.iter() {
        let ident = field.ident.as_ref().unwrap();
        if skip_id && ident == "id" { continue; }

        let field_name     = ident.to_string();
        let field_name_lit = syn::LitStr::new(&field_name, span);
        let table_name_str = table_lit.value();

        let (surreal_type, extras) = rust_type_to_surreal(
            &field_name, &field.ty, &table_name_str, &field_name, fk_map,
        );

        match surreal_type.as_str() {
            // ── Unknown struct (object) ──────────────────────────────────────
            "__dynamic__" => {
                let nested_type: syn::Type = syn::parse_str(&extras[0]).unwrap();
                dynamic_toks.push(quote! {
                    {
                        let resolved = <#nested_type as orm::model::SurrealType>::surreal_type();
                        parts.push(format!(
                            "DEFINE FIELD IF NOT EXISTS {} ON {} TYPE {}{};\n",
                            #field_name_lit,
                            #table_lit,
                            resolved,
                            if resolved == "object" { " FLEXIBLE" } else { "" },
                        ));
                        // structs return sub-field defs; enums return vec![]
                        parts.extend(
                            <#nested_type as orm::model::SurrealSchema>::nested_fields(
                                #table_lit,
                                #field_name_lit,
                            )
                        );
                    }
                });
            }

            // ── Option<unknown struct> ───────────────────────────────────────
            "__dynamic_option__" => {
                let nested_type: syn::Type = syn::parse_str(&extras[0]).unwrap();
                dynamic_toks.push(quote! {
                    {
                        let resolved = <#nested_type as orm::model::SurrealType>::surreal_type();
                        parts.push(format!(
                            "DEFINE FIELD IF NOT EXISTS {} ON {} TYPE option<{}>{};\n",
                            #field_name_lit,
                            #table_lit,
                            resolved,
                            if resolved == "object" { " FLEXIBLE" } else { "" },
                        ));
                        if resolved == "object" {
                            parts.extend(
                                <#nested_type as orm::model::SurrealSchema>::nested_fields(
                                    #table_lit,
                                    #field_name_lit,
                                )
                            );
                        }
                    }
                });
            }

            // ── Statically-known primitive/compound type ─────────────────────
            _ => {
                let is_flexible = surreal_type == "object"
                    || surreal_type == "option<object>";
                let suffix = if is_flexible { " FLEXIBLE" } else { "" };
                static_sql.push_str(&format!(
                    "DEFINE FIELD IF NOT EXISTS {} ON {} TYPE {}{};\n",
                    field_name, table_name_str, surreal_type, suffix,
                ));
            }
        }
    }

    (static_sql, dynamic_toks)
}

// ─────────────────────────────────────────────
//  #[derive(SurrealNested)]
//
//  Replaces the three old derives:
//    - SurrealTypeStruct
//    - SurrealTypeEnum
//    - SurrealSchema
//
//  Usage:
//    • On a struct  → SurrealType = "object", full SurrealSchema with nested fields
//    • On an enum   → SurrealType = "string", no-op SurrealSchema (enums are stored as strings)
//
//  You never need to write #[derive(SurrealTypeStruct/Enum/Schema)] again.
// ─────────────────────────────────────────────

#[proc_macro_derive(SurrealNested)]
pub fn derive_surreal_nested(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name  = &input.ident;

    match &input.data {
        // ── Struct → object ──────────────────────────────────────────────────
        syn::Data::Struct(data) => {
            let fields = &data.fields;
            let empty_fk_map = HashMap::new();
            let mut field_defs = vec![];

            for field in fields.iter() {
                let ident      = field.ident.as_ref().unwrap();
                let field_name = ident.to_string();
                let (surreal_type, _) = rust_type_to_surreal(
                    &field_name, &field.ty, "", &field_name, &empty_fk_map,
                );

                let is_flexible = surreal_type == "object"
                    || surreal_type == "option<object>";
                let suffix = if is_flexible { " FLEXIBLE" } else { "" };

                field_defs.push(quote! {
                    defs.push(format!(
                        "DEFINE FIELD IF NOT EXISTS {}.{} ON {} TYPE {}{};\n",
                        prefix, #field_name, table, #surreal_type, #suffix
                    ));
                });
            }

            quote! {
                impl orm::model::SurrealType for #name {
                    fn surreal_type() -> &'static str { "object" }
                }
                impl orm::model::SurrealSchema for #name {
                    fn nested_fields(table: &str, prefix: &str) -> Vec<String> {
                        let mut defs = Vec::new();
                        #(#field_defs)*
                        defs
                    }
                }
            }
            .into()
        }

        // ── Enum → string ────────────────────────────────────────────────────
        syn::Data::Enum(_) => {
            quote! {
                impl orm::model::SurrealType for #name {
                    fn surreal_type() -> &'static str { "string" }
                }
                impl orm::model::SurrealSchema for #name {}
            }
            .into()
        }

        _ => panic!("#[derive(SurrealNested)] only supports structs and enums"),
    }
}

// ─────────────────────────────────────────────
// #[derive(Model)]
// ─────────────────────────────────────────────

#[proc_macro_derive(Model, attributes(table, belongs_to, has_many, belongs_to_many, timestamp,soft_delete))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input       = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let span        = struct_name.span();

    // ── table name ──────────────────────────────────────────────────────────
    let mut table_name = to_snake_case(&struct_name.to_string());
    for attr in &input.attrs {
        if attr.path().is_ident("table") {
            if let Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let Lit::Str(lit_str) = &expr_lit.lit {
                        table_name = lit_str.value();
                    }
                }
            }
        }
    }
    let table_lit = syn::LitStr::new(&table_name, span);
 
    // ── fields ───────────────────────────────────────────────────────────────
    let fields = match &input.data {
        syn::Data::Struct(data) => &data.fields,
        _ => panic!("Model can only be derived for structs"),
    };
    fields.iter().find(|f| f.ident.as_ref().unwrap() == "id")
        .expect("Model requires an `id` field");

        // ── soft_delete flag ──
    let has_soft_delete = input.attrs.iter().any(|a| a.path().is_ident("soft_delete"));

    // Guard: if #[soft_delete] is present, enforce deleted_at field exists
    if has_soft_delete {
        fields
            .iter()
            .find(|f| f.ident.as_ref().unwrap() == "deleted_at")
            .expect("#[soft_delete] requires a `deleted_at: Option<Datetime>` field");
    }   
    // ── relations ────────────────────────────────────────────────────────────
    let relations = collect_relation_attrs(&input.attrs);

    // Build fk_map: field_name -> related_table
    // This feeds into rust_type_to_surreal so RecordId FKs get record<table> types.
    let fk_map = build_fk_map(&relations, fields);

    // ── schema SQL ───────────────────────────────────────────────────────────
    let mut static_fields = format!(
        "DEFINE TABLE IF NOT EXISTS {} SCHEMAFULL;\n", &table_name,
    );

    let (field_static_sql, dynamic_nested) =
        emit_schema_fields(fields, &table_lit, span, &fk_map, true);

    static_fields.push_str(&field_static_sql);
    let static_lit = syn::LitStr::new(&static_fields, span);

    // ── relation methods ─────────────────────────────────────────────────────
    let mut relation_methods    = vec![];
    let mut registration_calls  = vec![];

    for rel in &relations {
        match rel {
            RelationAttr::BelongsTo { path, fk, to } => {
                let parent_ident = &path.segments.last().unwrap().ident;
                let parent_snake = parent_ident.to_string().to_case(Case::Snake);

                let fn_str      = to.as_deref().unwrap_or(&parent_snake);
                let fn_name     = Ident::new(fn_str, span);
                let fn_name_lit = syn::LitStr::new(fn_str, span);

                let fk_str   = resolve_fk(&parent_snake, fields, fk.as_ref());
                let fk_field = Ident::new(&fk_str, span);

                // let related_table = to_snake_case(&parent_ident.to_string());

                relation_methods.push(quote! {
                    pub fn #fn_name<'a, R>(
                        &self,
                        repo: &'a orm::repository::Repo,
                    ) -> impl std::future::Future<Output = Result<Option<R>, orm::error::ErrorIO>>
                    where
                        R: serde::de::DeserializeOwned + surrealdb::types::SurrealValue,
                    {
                        let rel: orm::model::BelongsTo<'a, #path> =
                            orm::model::BelongsTo::new(repo, self.#fk_field.clone());
                        rel.one::<R>()
                    }
                });
                registration_calls.push(quote! {
                    orm::model::Relation::belongs_to_with_fk(
                        #table_lit,
                        #fn_name_lit,
                        <#path as orm::model::Model>::table_name(),
                        #fk_str,   // the resolved FK field name
                    );
                });
            }

            RelationAttr::HasMany { path, fk, to } => {
                let child_ident = &path.segments.last().unwrap().ident;
                let child_snake = child_ident.to_string().to_case(Case::Snake);
                let default_plural = pluralize(&child_snake, 2, false);

                // `to` overrides the method + registry key; default is pluralized child name
                let fn_str  = to.as_deref().unwrap_or(&default_plural);
                let fn_name = Ident::new(fn_str, span);
                let fn_lit  = syn::LitStr::new(fn_str, span);

                let parent_snake = struct_name.to_string().to_case(Case::Snake);
                // OLD: format!("{}_id", parent_snake)  ← Postgres-style
                // NEW: parent_snake directly           ← SurrealDB convention (table name = FK column)
                let fk_str = fk.clone().unwrap_or_else(|| parent_snake.clone());
                let fk_lit = syn::LitStr::new(&fk_str, span);

                relation_methods.push(quote! {
                    pub fn #fn_name<'a>(
                        &self,
                        repo: &'a orm::repository::Repo,
                    ) -> orm::model::HasMany<'a, #path> {
                        orm::model::HasMany::new(repo, #fk_str, self.id.clone())
                    }
                });

                registration_calls.push(quote! {
                    orm::model::Relation::has_many_with_fk(
                        #table_lit,
                        #fn_lit,
                        <#path as orm::model::Model>::table_name(),
                        #fk_lit,
                    );
                });
            }

            RelationAttr::BelongsToMany { related, pivot, is_left } => {
                let fn_name     = relation_plural_fn_name_from_path(related, span);
                let fn_name_lit = syn::LitStr::new(&fn_name.to_string(), span);
                let is_left_tok = if *is_left { quote! { true } } else { quote! { false } };

                relation_methods.push(quote! {
                    pub fn #fn_name<'a>(
                        &self,
                        repo: &'a orm::repository::Repo,
                    ) -> orm::model::BelongsToMany<'a, #pivot, #related, Self> {
                        orm::model::BelongsToMany::new(repo, self.id.clone(), #is_left_tok)
                    }
                });

                registration_calls.push(quote! {
                    orm::model::Relation::belongs_to_many(
                        #table_lit,
                        #fn_name_lit,
                        <#related as orm::model::Model>::table_name(),
                        <#pivot  as orm::model::Model>::table_name(),
                        <#pivot  as orm::model::Pivot>::left_key(),
                        <#pivot  as orm::model::Pivot>::right_key(),
                        #is_left_tok,
                    );
                });
            }
        }
    }
    // ── instance methods on the struct ──────────────────────────
    let delete_methods = if has_soft_delete {
        quote! {
            /// Soft-delete: sets deleted_at to now, checks relations first.
            pub async fn delete(
                self,
                repo: &orm::repository::Repo,
            ) -> Result<Self, orm::error::ErrorIO> {
                // Relation guard
                Self::check_no_dependents(repo, &self.id).await?;

                let query = orm::model::Query::<Self, orm::model::query::Update>::new(repo);
                query
                    .find(self.id.clone())
                    .set("deleted_at", chrono::Utc::now().to_rfc3339()) // now
                    .exec::<Self>()
                    .await
            }

            /// Hard-delete: permanently removes, checks relations first.
            pub async fn force_delete(
                self,
                repo: &orm::repository::Repo,
            ) -> Result<usize, orm::error::ErrorIO> {
                Self::check_no_dependents(repo, &self.id).await?;

                let query = orm::model::Query::<Self, orm::model::query::Delete>::new(repo);
                query.find(self.id.clone()).exec().await
            }
            fn soft_delete() -> bool { true }
            pub async fn restore<R>(
                self,
                repo: &orm::repository::Repo,
            ) -> Result<R, orm::error::ErrorIO>
                where R: serde::de::DeserializeOwned + surrealdb::types::SurrealValue,
            {
                let sql = format!(
                    "UPDATE {} SET deleted_at = NONE WHERE id = $id",
                    #table_lit
                );
                let mut res = repo.db
                    .query(sql)
                    .bind(("id", self.id.clone()))
                    .await
                    .map_err(orm::error::ErrorIO::from)?;

                let record: Option<R> = res.take(0).map_err(orm::error::ErrorIO::from)?;
                record.ok_or_else(|| orm::error::ErrorIO::NotFound(
                    format!("{:?}:{:?} not found or already restored", self.id.table, self.id.key)
                ))
            }

            pub async fn trashed<R>(
                repo: &orm::repository::Repo,
            ) -> Result<Vec<R>, orm::error::ErrorIO>
                where R: serde::de::DeserializeOwned + surrealdb::types::SurrealValue,
            {
                let sql = format!(
                    "SELECT * FROM {} WHERE deleted_at IS NOT NULL",
                    #table_lit
                );
                let mut res = repo.db
                    .query(sql)
                    .await
                    .map_err(orm::error::ErrorIO::from)?;

                Ok(res.take(0).map_err(orm::error::ErrorIO::from)?)
            }

            pub async fn find_trashed<R>(
                repo: &orm::repository::Repo,
                id: surrealdb::types::RecordId,
            ) -> Result<R, orm::error::ErrorIO>
                where R: serde::de::DeserializeOwned + surrealdb::types::SurrealValue,
            {
                let sql = format!(
                    "SELECT * FROM {} WHERE id = $id AND deleted_at IS NOT NULL LIMIT 1",
                    #table_lit
                );
                let mut res = repo.db
                    .query(sql)
                    .bind(("id", id.clone()))
                    .await
                    .map_err(orm::error::ErrorIO::from)?;

                let record: Option<R> = res.take(0).map_err(orm::error::ErrorIO::from)?;
                record.ok_or_else(|| orm::error::ErrorIO::NotFound(
                    format!("{:?}:{:?} not found or already restored", id.table, id.key)
                ))
            }

        }
    } else {
        quote! {
            /// Hard-delete: permanently removes, checks relations first.
            pub async fn delete(
                self,
                repo: &orm::repository::Repo,
            ) -> Result<usize, orm::error::ErrorIO> {
                Self::check_no_dependents(repo, &self.id).await?;

                let query = orm::model::Query::<Self, orm::model::query::Delete>::new(repo);
                query.find(self.id.clone()).exec().await
            }
        }
    };

    // ── static helpers ───────────────────────────────────────────
    let soft_delete_scope = if has_soft_delete {
        quote! {
            /// Returns the extra WHERE clause to exclude soft-deleted rows.
            /// Injected automatically by select/update/destroy queries.
            pub fn active_scope() -> &'static str {
                "deleted_at IS NULL"
            }
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        impl #struct_name {
            // ── CRUD ──────────────────────────────────────────────────────────
            pub fn insert<'a>(repo: &'a orm::repository::Repo)
                -> orm::model::Query<'a, Self, orm::model::query::Insert>
            {
                orm::model::Query::<Self, orm::model::query::Insert>::new(repo)
            }
            pub fn select<'a>(repo: &'a orm::repository::Repo)
                -> orm::model::Query<'a, Self, orm::model::query::Select>
            {
                orm::model::Query::<Self, orm::model::query::Select>::new(repo)
            }
            pub fn update<'a>(repo: &'a orm::repository::Repo)
                -> orm::model::Query<'a, Self, orm::model::query::Update>
            {
                orm::model::Query::<Self, orm::model::query::Update>::new(repo)
            }
            pub fn destroy<'a>(repo: &'a orm::repository::Repo)
                -> orm::model::Query<'a, Self, orm::model::query::Delete>
            {
                orm::model::Query::<Self, orm::model::query::Delete>::new(repo)
            }
            pub fn save<'a>(self, repo: &'a orm::repository::Repo)
                -> impl std::future::Future<Output = Result<Self, orm::error::ErrorIO>> + 'a
            {
                let query = orm::model::Query::<Self, orm::model::query::Update>::new(repo);
                query.find(self.id.clone()).values(self).exec::<Self>()
            }
            #delete_methods
            #soft_delete_scope
            // ── Relation methods ──────────────────────────────────────────────
            #(#relation_methods)*

            // ── Eager registration ────────────────────────────────────────────
            pub fn register_relations() {
                #(#registration_calls)*
            }
        }

        impl orm::model::Model for #struct_name {
            fn table_name() -> &'static str { #table_lit }
            fn id(&self) -> surrealdb::types::RecordId { self.id.clone() }
            fn schema() -> String {
                let mut parts = vec![#static_lit.to_string()];
                #(#dynamic_nested)*
                parts.join("")
            }
            fn check_no_dependents<'a>(
                repo: &'a orm::repository::Repo,
                id: &'a surrealdb::types::RecordId,
            ) -> impl std::future::Future<Output = Result<(), orm::error::ErrorIO>> + 'a {
                async move {
                    use orm::model::relations::{Relation, RelationType};

                    let relations = Relation::get_all(#table_lit);
                    for rel in relations {
                        match rel.relation_type {
                            RelationType::HasMany => {
                                let child_table = rel.child_table.as_deref().unwrap_or_default();
                                let fk = rel.fk.as_deref().unwrap_or_default();
                                if child_table.is_empty() || fk.is_empty() { continue; }

                                let sql = format!(
                                    "SELECT count() FROM {} WHERE {} = $id GROUP ALL",
                                    child_table, fk
                                );
                                let mut res = repo.db
                                    .query(sql)
                                    .bind(("id", id.clone()))
                                    .await
                                    .map_err(orm::error::ErrorIO::from)?;

                                let count: Option<i64> = res.take("count").map_err(orm::error::ErrorIO::from)?;
                                if count.unwrap_or(0) > 0 {
                                    return Err(orm::error::ErrorIO::Conflict(format!(
                                        "Cannot delete: {} record(s) in `{}` depend on this item (via `{}`).",
                                        count.unwrap_or(0), child_table, fk
                                    )));
                                }
                            }
                            RelationType::BelongsToMany => {
                                let pivot = rel.pivot.as_deref().unwrap_or_default();
                                let fk = if rel.is_left.unwrap_or(true) {
                                    rel.pivot_left_key.as_deref().unwrap_or_default()
                                } else {
                                    rel.pivot_right_key.as_deref().unwrap_or_default()
                                };
                                if pivot.is_empty() || fk.is_empty() { continue; }

                                let sql = format!(
                                    "SELECT count() FROM {} WHERE {} = $id GROUP ALL",
                                    pivot, fk
                                );
                                let mut res = repo.db
                                    .query(sql)
                                    .bind(("id", id.clone()))
                                    .await
                                    .map_err(orm::error::ErrorIO::from)?;

                                let count: Option<i64> = res.take("count").map_err(orm::error::ErrorIO::from)?;
                                if count.unwrap_or(0) > 0 {
                                    return Err(orm::error::ErrorIO::Conflict(format!(
                                        "Cannot delete: {} pivot record(s) in `{}` reference this item.",
                                        count.unwrap_or(0), pivot
                                    )));
                                }
                            }
                            RelationType::BelongsTo => {}
                        }
                    }
                    Ok(())
                }
            }
        }
    };

    TokenStream::from(expanded)
}

// ─────────────────────────────────────────────
// #[derive(PivotModel)]
// ─────────────────────────────────────────────

// #[proc_macro_derive(PivotModel, attributes(left, right, belongs_to, table, timestamp))]
// pub fn pivot_model_derive(input: TokenStream) -> TokenStream {
//     let input       = parse_macro_input!(input as DeriveInput);
//     let struct_name = input.ident.clone();
//     let span        = struct_name.span();
//     let mut table_name = struct_name.to_string().to_case(Case::Snake);
//     let table_lit = syn::LitStr::new(&table_name, span);
//     let fields = match &input.data {
//         Data::Struct(data) => match &data.fields {
//             Fields::Named(named) => named.named.iter().collect::<Vec<_>>(),
//             _ => panic!("PivotModel requires named fields"),
//         },
//         _ => panic!("PivotModel can only be derived for structs"),
//     };

//     // ── detect #[left] / #[right] ────────────────────────────────────────────
//     let mut left_field  = None;
//     let mut right_field = None;
//     for field in &fields {
//         for attr in &field.attrs {
//             if attr.path().is_ident("left")  { left_field  = Some(field.ident.clone().unwrap()); }
//             if attr.path().is_ident("right") { right_field = Some(field.ident.clone().unwrap()); }
//         }
//     }
//     let left_ident  = left_field.expect("Missing #[left] field");
//     let right_ident = right_field.expect("Missing #[right] field");

//     let has_timestamp = input.attrs.iter().any(|attr| attr.path().is_ident("timestamp"));

//     // ── extra fields ─────────────────────────────────────────────────────────
//     let extra_fields: Vec<(syn::Ident, &syn::Type)> = fields.iter().filter_map(|f| {
//         let ident = f.ident.clone().unwrap();
//         let ty    = &f.ty;
//         if ident != left_ident
//             && ident != right_ident
//             && ident != "id"
//             && !(has_timestamp && (ident == "created_at" || ident == "updated_at"))
//         {
//             Some((ident, ty))
//         } else {
//             None
//         }
//     }).collect();

//     let extra_type = if extra_fields.is_empty() {
//         quote! { () }
//     } else {
//         let types = extra_fields.iter().map(|(_, ty)| quote! { #ty });
//         quote! { ( #( #types ),* ) }
//     };

//     let extra_destructure = if extra_fields.is_empty() {
//         quote! {}
//     } else {
//         let idents = extra_fields.iter().map(|(id, _)| id);
//         quote! { let ( #( #idents ),* ) = extra; }
//     };

//     let extra_assignments = extra_fields.iter().map(|(id, _)| quote! { #id: #id });

//     let timestamp_init   = if has_timestamp {
//         quote! { let now = chrono::Utc::now().to_rfc3339(); }
//     } else {
//         quote! {}
//     };
//     let timestamp_assign = if has_timestamp {
//         quote! { created_at: now.clone(), updated_at: now, }
//     } else {
//         quote! {}
//     };

//     // ── table override ────────────────────────────────────────────────────────
//     for attr in &input.attrs {
//         if attr.path().is_ident("table") {
//             if let Meta::NameValue(meta) = &attr.meta {
//                 if let syn::Expr::Lit(expr_lit) = &meta.value {
//                     if let Lit::Str(lit_str) = &expr_lit.lit {
//                         table_name = lit_str.value();
//                     }
//                 }
//             }
//         }
//     }

//     // ── schema SQL — use emit_schema_fields (empty fk_map for pivots) ────────
//     let empty_fk_map  = HashMap::new();
//     // Inline pivot SQL (all pivot fields are statically typed — no dynamic sentinels expected)
//     let mut pivot_sql = format!("DEFINE TABLE IF NOT EXISTS {} SCHEMAFULL;\n", table_name);
//     for field in &fields {
//         let ident = field.ident.as_ref().unwrap();
//         if ident == "id" { continue; }
//         let field_name = ident.to_string();
//         let (surreal_type, _) = rust_type_to_surreal(
//             &field_name, &field.ty, &table_name, &field_name, &empty_fk_map,
//         );
//         pivot_sql.push_str(&format!(
//             "DEFINE FIELD IF NOT EXISTS {} ON {} TYPE {};\n",
//             field_name, table_name, surreal_type,
//         ));
//     }

//     let migration_literal = syn::LitStr::new(&pivot_sql, proc_macro2::Span::call_site());
//     let table_literal     = syn::LitStr::new(&table_name, proc_macro2::Span::call_site());
//     let expanded = quote! {
//         impl orm::model::Model for #struct_name {
//             fn table_name() -> &'static str { #table_literal }
//             fn id(&self) -> surrealdb::types::RecordId { self.id.clone() }
//             fn schema() -> String { #migration_literal.to_string() }
//             fn check_no_dependents<'a>(
//                 repo: &'a orm::repository::Repo,
//                 id: &'a surrealdb::types::RecordId,
//             ) -> impl std::future::Future<Output = Result<(), orm::error::ErrorIO>> + 'a {
//                 async move {
//                     use orm::model::relations::{Relation, RelationType};

//                     let relations = Relation::get_all(#table_lit);
//                     for rel in relations {
//                         match rel.relation_type {
//                             RelationType::HasMany => {
//                                 let child_table = rel.child_table.as_deref().unwrap_or_default();
//                                 let fk = rel.fk.as_deref().unwrap_or_default();
//                                 if child_table.is_empty() || fk.is_empty() { continue; }

//                                 let sql = format!(
//                                     "SELECT count() FROM {} WHERE {} = $id GROUP ALL",
//                                     child_table, fk
//                                 );
//                                 let mut res = repo.db
//                                     .query(sql)
//                                     .bind(("id", id.clone()))
//                                     .await
//                                     .map_err(orm::error::ErrorIO::from)?;

//                                 let count: Option<i64> = res.take("count").map_err(orm::error::ErrorIO::from)?;
//                                 if count.unwrap_or(0) > 0 {
//                                     return Err(orm::error::ErrorIO::Conflict(format!(
//                                         "Cannot delete: {} record(s) in `{}` depend on this item (via `{}`).",
//                                         count.unwrap_or(0), child_table, fk
//                                     )));
//                                 }
//                             }
//                             RelationType::BelongsToMany => {
//                                 let pivot = rel.pivot.as_deref().unwrap_or_default();
//                                 let fk = if rel.is_left.unwrap_or(true) {
//                                     rel.pivot_left_key.as_deref().unwrap_or_default()
//                                 } else {
//                                     rel.pivot_right_key.as_deref().unwrap_or_default()
//                                 };
//                                 if pivot.is_empty() || fk.is_empty() { continue; }

//                                 let sql = format!(
//                                     "SELECT count() FROM {} WHERE {} = $id GROUP ALL",
//                                     pivot, fk
//                                 );
//                                 let mut res = repo.db
//                                     .query(sql)
//                                     .bind(("id", id.clone()))
//                                     .await
//                                     .map_err(orm::error::ErrorIO::from)?;

//                                 let count: Option<i64> = res.take("count").map_err(orm::error::ErrorIO::from)?;
//                                 if count.unwrap_or(0) > 0 {
//                                     return Err(orm::error::ErrorIO::Conflict(format!(
//                                         "Cannot delete: {} pivot record(s) in `{}` reference this item.",
//                                         count.unwrap_or(0), pivot
//                                     )));
//                                 }
//                             }
//                             RelationType::BelongsTo => {}
//                         }
//                     }
//                     Ok(())
//                 }
//             }
//         }

//         impl orm::model::Pivot for #struct_name {
//             type Extra = #extra_type;

//             fn left_key()  -> &'static str { stringify!(#left_ident) }
//             fn right_key() -> &'static str { stringify!(#right_ident) }

//             fn left_id(&self)  -> surrealdb::types::RecordId { self.#left_ident.clone() }
//             fn right_id(&self) -> surrealdb::types::RecordId { self.#right_ident.clone() }

//             fn new(
//                 left:  surrealdb::types::RecordId,
//                 right: surrealdb::types::RecordId,
//                 extra: #extra_type,
//             ) -> Self {
//                 #extra_destructure
//                 #timestamp_init
//                 Self {
//                     id: surrealdb::types::RecordId {
//                         table: Self::table_name().into(),
//                         key:   surrealdb::types::RecordIdKey::String(
//                             surrealdb::types::Uuid::new_v4().to_string()
//                         ),
//                     },
//                     #left_ident:  left,
//                     #right_ident: right,
//                     #( #extra_assignments, )*
//                     #timestamp_assign
//                 }
//             }
//         }
//     };

//     TokenStream::from(expanded)
// }

#[proc_macro_derive(PivotModel, attributes(left, right, belongs_to, table, timestamp))]
pub fn pivot_model_derive(input: TokenStream) -> TokenStream {
    let input       = parse_macro_input!(input as DeriveInput);
    let struct_name = input.ident.clone();
    let span        = struct_name.span();
    let mut table_name = struct_name.to_string().to_case(Case::Snake);

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => named.named.iter().collect::<Vec<_>>(),
            _ => panic!("PivotModel requires named fields"),
        },
        _ => panic!("PivotModel can only be derived for structs"),
    };

    // ── detect #[left] / #[right] ────────────────────────────────────────────
    let mut left_field  = None;
    let mut right_field = None;
    for field in &fields {
        for attr in &field.attrs {
            if attr.path().is_ident("left")  { left_field  = Some(field.ident.clone().unwrap()); }
            if attr.path().is_ident("right") { right_field = Some(field.ident.clone().unwrap()); }
        }
    }
    let left_ident  = left_field.expect("Missing #[left] field");
    let right_ident = right_field.expect("Missing #[right] field");

    // ── infer belongs_to struct names from field names ────────────────────────
    // "article"            → Article
    // "application_method" → ApplicationMethod
    let left_pascal  = left_ident.to_string().to_case(Case::Pascal);
    let right_pascal = right_ident.to_string().to_case(Case::Pascal);
    let left_path: syn::Path  = syn::parse_str(&left_pascal)
        .unwrap_or_else(|_| panic!("Could not parse `{}` as a path", left_pascal));
    let right_path: syn::Path = syn::parse_str(&right_pascal)
        .unwrap_or_else(|_| panic!("Could not parse `{}` as a path", right_pascal));

    let left_str  = left_ident.to_string();
    let right_str = right_ident.to_string();
    let left_lit  = syn::LitStr::new(&left_str, span);
    let right_lit = syn::LitStr::new(&right_str, span);

    let left_fn  = syn::Ident::new(&left_str, span);
    let right_fn = syn::Ident::new(&right_str, span);

    let has_timestamp = input.attrs.iter().any(|attr| attr.path().is_ident("timestamp"));

    // ── extra fields ─────────────────────────────────────────────────────────
    let extra_fields: Vec<(syn::Ident, &syn::Type)> = fields.iter().filter_map(|f| {
        let ident = f.ident.clone().unwrap();
        let ty    = &f.ty;
        if ident != left_ident
            && ident != right_ident
            && ident != "id"
            && !(has_timestamp && (ident == "created_at" || ident == "updated_at"))
        {
            Some((ident, ty))
        } else {
            None
        }
    }).collect();

    let extra_type = if extra_fields.is_empty() {
        quote! { () }
    } else {
        let types = extra_fields.iter().map(|(_, ty)| quote! { #ty });
        quote! { ( #( #types ),* ) }
    };

    let extra_destructure = if extra_fields.is_empty() {
        quote! {}
    } else {
        let idents = extra_fields.iter().map(|(id, _)| id);
        quote! { let ( #( #idents ),* ) = extra; }
    };

    let extra_assignments: Vec<_> = extra_fields.iter()
        .map(|(id, _)| quote! { #id: #id })
        .collect();

    let timestamp_init = if has_timestamp {
        quote! { let now = chrono::Utc::now().to_rfc3339(); }
    } else {
        quote! {}
    };
    let timestamp_assign = if has_timestamp {
        quote! { created_at: now.clone(), updated_at: now, }
    } else {
        quote! {}
    };

    // ── table override ────────────────────────────────────────────────────────
    for attr in &input.attrs {
        if attr.path().is_ident("table") {
            if let Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let Lit::Str(lit_str) = &expr_lit.lit {
                        table_name = lit_str.value();
                    }
                }
            }
        }
    }

    // ── schema SQL ────────────────────────────────────────────────────────────
    let empty_fk_map = HashMap::new();
    let mut pivot_sql = format!("DEFINE TABLE IF NOT EXISTS {} SCHEMAFULL;\n", table_name);
    for field in &fields {
        let ident = field.ident.as_ref().unwrap();
        if ident == "id" { continue; }
        let field_name = ident.to_string();
        let (surreal_type, _extras) = rust_type_to_surreal(
            &field_name, &field.ty, &table_name, &field_name, &empty_fk_map,
        );

        // skip sentinels — dynamic types (enums/structs) are resolved at runtime
        // via SurrealType; we emit them as `string` as a safe static fallback
        let resolved_type = match surreal_type.as_str() {
            "__dynamic__"         => "string".to_string(),
            "__dynamic_option__"  => "option<string>".to_string(),
            other                 => other.to_string(),
        };

        let is_flexible = resolved_type == "object" || resolved_type == "option<object>";
        if is_flexible {
            pivot_sql.push_str(&format!(
                "DEFINE FIELD IF NOT EXISTS {} ON {} FLEXIBLE TYPE {};\n",
                field_name, table_name, resolved_type,
            ));
        } else {
            pivot_sql.push_str(&format!(
                "DEFINE FIELD IF NOT EXISTS {} ON {} TYPE {};\n",
                field_name, table_name, resolved_type,
            ));
        }
    }

    let migration_literal = syn::LitStr::new(&pivot_sql, span);
    // let table_literal     = syn::LitStr::new(&table_name, span);

    // ── conditional new() — only when Extra = () ─────────────────────────────
    let is_unit_extra = extra_fields.is_empty();
    let _new_method = if is_unit_extra {
        quote! {
            fn new(left: surrealdb::types::RecordId, right: surrealdb::types::RecordId) -> Self {
                Self::new_with(left, right, ())
            }
        }
    } else {
        quote! {}
    };
    let table_literal = syn::LitStr::new(&table_name, span);
    let table_lit     = syn::LitStr::new(&table_name, span);
    let expanded = quote! {
        impl #struct_name {
            // ── auto belongs_to for left field ────────────────────────────────
            pub fn #left_fn<'a, R>(
                &self,
                repo: &'a orm::repository::Repo,
            ) -> impl std::future::Future<Output = Result<Option<R>, orm::error::ErrorIO>>
            where
                R: serde::de::DeserializeOwned + surrealdb::types::SurrealValue,
            {
                let rel = orm::model::BelongsTo::<'a, #left_path>::new(repo, self.#left_ident.clone());
                rel.one::<R>()
            }

            // ── auto belongs_to for right field ───────────────────────────────
            pub fn #right_fn<'a, R>(
                &self,
                repo: &'a orm::repository::Repo,
            ) -> impl std::future::Future<Output = Result<Option<R>, orm::error::ErrorIO>>
            where
                R: serde::de::DeserializeOwned + surrealdb::types::SurrealValue,
            {
                let rel = orm::model::BelongsTo::<'a, #right_path>::new(repo, self.#right_ident.clone());
                rel.one::<R>()
            }

            pub fn register_relations() {
                orm::model::Relation::belongs_to_with_fk(
                    #table_literal,
                    #left_lit,
                    <#left_path as orm::model::Model>::table_name(),
                    #left_lit,
                );
                orm::model::Relation::belongs_to_with_fk(
                    #table_literal,
                    #right_lit,
                    <#right_path as orm::model::Model>::table_name(),
                    #right_lit,
                );
            }
        }

        impl orm::model::Model for #struct_name {
            fn table_name() -> &'static str { #table_literal }
            fn id(&self) -> surrealdb::types::RecordId { self.id.clone() }
            fn schema() -> String { #migration_literal.to_string() }
            fn check_no_dependents<'a>(
                repo: &'a orm::repository::Repo,
                id: &'a surrealdb::types::RecordId,
            ) -> impl std::future::Future<Output = Result<(), orm::error::ErrorIO>> + 'a {
                async move {
                    use orm::model::relations::{Relation, RelationType};
                    let relations = Relation::get_all(#table_lit);
                    for rel in relations {
                        match rel.relation_type {
                            RelationType::HasMany => {
                                let child_table = rel.child_table.as_deref().unwrap_or_default();
                                let fk = rel.fk.as_deref().unwrap_or_default();
                                if child_table.is_empty() || fk.is_empty() { continue; }
                                let sql = format!(
                                    "SELECT count() FROM {} WHERE {} = $id GROUP ALL",
                                    child_table, fk
                                );
                                let mut res = repo.db
                                    .query(sql)
                                    .bind(("id", id.clone()))
                                    .await
                                    .map_err(orm::error::ErrorIO::from)?;
                                let count: Option<i64> = res.take("count").map_err(orm::error::ErrorIO::from)?;
                                if count.unwrap_or(0) > 0 {
                                    return Err(orm::error::ErrorIO::Conflict(format!(
                                        "Cannot delete: {} record(s) in `{}` depend on this item (via `{}`).",
                                        count.unwrap_or(0), child_table, fk
                                    )));
                                }
                            }
                            RelationType::BelongsToMany => {
                                let pivot = rel.pivot.as_deref().unwrap_or_default();
                                let fk = if rel.is_left.unwrap_or(true) {
                                    rel.pivot_left_key.as_deref().unwrap_or_default()
                                } else {
                                    rel.pivot_right_key.as_deref().unwrap_or_default()
                                };
                                if pivot.is_empty() || fk.is_empty() { continue; }
                                let sql = format!(
                                    "SELECT count() FROM {} WHERE {} = $id GROUP ALL",
                                    pivot, fk
                                );
                                let mut res = repo.db
                                    .query(sql)
                                    .bind(("id", id.clone()))
                                    .await
                                    .map_err(orm::error::ErrorIO::from)?;
                                let count: Option<i64> = res.take("count").map_err(orm::error::ErrorIO::from)?;
                                if count.unwrap_or(0) > 0 {
                                    return Err(orm::error::ErrorIO::Conflict(format!(
                                        "Cannot delete: {} pivot record(s) in `{}` reference this item.",
                                        count.unwrap_or(0), pivot
                                    )));
                                }
                            }
                            RelationType::BelongsTo => {}
                        }
                    }
                    Ok(())
                }
            }
        }

        impl orm::model::Pivot for #struct_name {
            type Extra = #extra_type;

            fn left_key()  -> &'static str { stringify!(#left_ident) }
            fn right_key() -> &'static str { stringify!(#right_ident) }

            fn left_id(&self)  -> surrealdb::types::RecordId { self.#left_ident.clone() }
            fn right_id(&self) -> surrealdb::types::RecordId { self.#right_ident.clone() }

            fn new(
                left:  surrealdb::types::RecordId,
                right: surrealdb::types::RecordId,
                extra: #extra_type,
            ) -> Self {
                #extra_destructure
                #timestamp_init
                Self {
                    id: surrealdb::types::RecordId {
                        table: Self::table_name().into(),
                        key: surrealdb::types::RecordIdKey::String(
                            surrealdb::types::Uuid::new_v4().to_string()
                        ),
                    },
                    #left_ident:  left,
                    #right_ident: right,
                    #( #extra_assignments, )*
                    #timestamp_assign
                }
            }
        }
    };

    TokenStream::from(expanded)
}


// ─────────────────────────────────────────────────────────────
//  Backward-compat shims (deprecated — prefer SurrealNested)
//  Remove once all call-sites are migrated.
// ─────────────────────────────────────────────────────────────

/// @deprecated  Use `#[derive(SurrealNested)]` instead.
#[proc_macro_derive(SurrealTypeStruct)]
pub fn derive_surreal_type_struct(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name  = &input.ident;
    quote! {
        impl orm::model::SurrealType for #name {
            fn surreal_type() -> &'static str { "object" }
        }
    }.into()
}

/// @deprecated  Use `#[derive(SurrealNested)]` instead.
#[proc_macro_derive(SurrealTypeEnum)]
pub fn derive_surreal_type_enum(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name  = &input.ident;
    quote! {
        impl orm::model::SurrealType for #name {
            fn surreal_type() -> &'static str { "string" }
        }
        impl orm::model::SurrealSchema for #name {}
    }.into()
}

/// @deprecated  Use `#[derive(SurrealNested)]` instead.
#[proc_macro_derive(SurrealSchema)]
pub fn derive_surreal_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name  = &input.ident;
    let fields = match &input.data {
        syn::Data::Struct(data) => &data.fields,
        _ => panic!("SurrealSchema only works on structs"),
    };
    let empty_fk_map = HashMap::new();
    let mut field_defs = vec![];

    for field in fields.iter() {
        let ident      = field.ident.as_ref().unwrap();
        let field_name = ident.to_string();
        let (surreal_type, _) = rust_type_to_surreal(
            &field_name, &field.ty, "", &field_name, &empty_fk_map,
        );
        let is_flexible = surreal_type == "object" || surreal_type == "option<object>";
        let suffix      = if is_flexible { " FLEXIBLE" } else { "" };
        field_defs.push(quote! {
            defs.push(format!(
                "DEFINE FIELD IF NOT EXISTS {}.{} ON {} TYPE {}{};\n",
                prefix, #field_name, table, #surreal_type, #suffix
            ));
        });
    }

    quote! {
        impl orm::model::SurrealSchema for #name {
            fn nested_fields(table: &str, prefix: &str) -> Vec<String> {
                let mut defs = Vec::new();
                #(#field_defs)*
                defs
            }
        }
    }.into()
}
