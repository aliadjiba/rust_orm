use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, DeriveInput, Meta, Lit,
    parse::{Parse, ParseStream},
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


/// Returns (surreal_type, extra_field_definitions)
fn rust_type_to_surreal(
    field_name: &str,
    ty: &Type,
    table_name: &str,
    field_prefix: &str,
) -> (String, Vec<String>) {
    match ty {
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last().unwrap();
            let ident = segment.ident.to_string();
            match ident.as_str() {
                "String" => ("string".to_string(), vec![]),
                "bool" => ("bool".to_string(), vec![]),
                "i8" | "i16" | "i32" | "i64" |
                "u8" | "u16" | "u32" | "u64" |
                "usize" | "isize" => ("int".to_string(), vec![]),
                "f32" | "f64" => ("float".to_string(), vec![]),
                "RecordId" => {
                    if field_name == "id" {
                        ("record".to_string(), vec![])
                    } else {
                        (format!("record<{}>", field_name), vec![])
                    }
                }
                "Option" => {
                    if let PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            let (inner_type, extras) =
                                rust_type_to_surreal(field_name, inner, table_name, field_prefix);
                            if inner_type == "__dynamic__" {
                                // propagate the sentinel with option wrapper signal
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
                                rust_type_to_surreal(field_name, inner, table_name, field_prefix);
                            return (format!("array<{}>", inner_type), extra);
                        }
                    }
                    ("array".to_string(), vec![])
                }
                // Unknown type → treat as a nested object, look up its fields via SurrealSchema
                _ => {
                    // We can't introspect the struct at macro time unless we have a trait.
                    // Instead, we use the SurrealSchema trait (see below) via a compile-time
                    // indirection. Here we emit "object" + a sentinel that the caller replaces
                    // with the trait call at code-generation time.
                    // ("object".to_string(), vec![ident.clone()])
                    // ("object FLEXIBLE".to_string(), vec![])
                    // ("object".to_string(), vec![])
                    ("__dynamic__".to_string(), vec![ident.clone()])
                }
            }
        }
        _ => ("any".to_string(), vec![]),
    }
}


// ─────────────────────────────────────────────
// Relation attribute data collected at parse time
// ─────────────────────────────────────────────

enum RelationAttr {
    BelongsTo(Path),
    HasMany(Path),
    BelongsToMany { related: Path, pivot: Path, is_left: bool },
}

fn collect_relation_attrs(attrs: &[syn::Attribute]) -> Vec<RelationAttr> {
    let mut out = vec![];
    for attr in attrs {
        let name = attr.path().get_ident().map(|i| i.to_string());
        match name.as_deref() {
            Some("belongs_to") => {
                if let Ok(syn::Meta::List(list)) = attr.meta.clone().try_into() {
                    if let Ok(path) = syn::parse2::<Path>(list.tokens.clone()) {
                        out.push(RelationAttr::BelongsTo(path));
                    }
                }
            }
            Some("has_many") => {
                if let Ok(syn::Meta::List(list)) = attr.meta.clone().try_into() {
                    if let Ok(path) = syn::parse2::<Path>(list.tokens.clone()) {
                        out.push(RelationAttr::HasMany(path));
                    }
                }
            }
            Some("belongs_to_many") => {
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
                                pivot: pivot.clone(),
                                is_left,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

// ─────────────────────────────────────────────
// Model derive — generates methods + Model impl + relation methods + register_relations
// ─────────────────────────────────────────────

#[proc_macro_derive(Model, attributes(table, belongs_to, has_many, belongs_to_many, timestamp))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let span = struct_name.span();

    // ── table name ──
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
    let mut static_fields = String::new();
    let mut dynamic_nested: Vec<proc_macro2::TokenStream> = vec![];
    static_fields.push_str(&format!(
        "DEFINE TABLE IF NOT EXISTS {} SCHEMAFULL;\n", &table_name
    ));
    // ── fields ──
    let fields = match &input.data {
        syn::Data::Struct(data) => &data.fields,
        _ => panic!("Model can only be derived for structs"),
    };
    fields.iter().find(|f| f.ident.as_ref().unwrap() == "id")
        .expect("Model requires an `id` field");

    // ── schema SQL ──
    // let mut migration_sql = format!("DEFINE TABLE IF NOT EXISTS {} SCHEMAFULL;\n", table_name);
    for field in fields.iter() {
        let ident = field.ident.as_ref().unwrap();
        if ident == "id" { continue; }
        let field_name = ident.to_string();
        let field_name_lit = syn::LitStr::new(&field_name, span);

        let (surreal_type, extras) = rust_type_to_surreal(
            &field_name, &field.ty, &table_name, &field_name
        );
        if surreal_type == "__dynamic__" {
    // type is unknown at macro time — resolve via SurrealType trait at runtime
    let nested_type: syn::Type = syn::parse_str(&extras[0]).unwrap();
    // dynamic_nested.push(quote! {
    //     {
    //         let resolved = <#nested_type as orm::model::SurrealType>::surreal_type();
    //         parts.push(format!(
    //             "DEFINE FIELD IF NOT EXISTS {} ON {} {} TYPE {};\n",
    //             #field_name_lit,
    //             #table_lit,
    //             if resolved == "object" { "FLEXIBLE" } else { "" },
    //             resolved,
    //         ));
    //         if resolved == "object" {
    //             parts.extend(
    //                 <#nested_type as orm::model::SurrealSchema>::nested_fields(
    //                     #table_lit,
    //                     #field_name_lit,
    //                 )
    //             );
    //         }
    //     }
    //     });
    dynamic_nested.push(quote! {
    {
        let resolved = <#nested_type as orm::model::SurrealType>::surreal_type();
        parts.push(format!(
            "DEFINE FIELD IF NOT EXISTS {} ON {} TYPE {} {};\n",
            #field_name_lit,
            #table_lit,
            resolved,
            if resolved == "object" { "FLEXIBLE" } else { "" },
        ));
        // safe for all types — enums return vec![], structs return field defs
        parts.extend(
            <#nested_type as orm::model::SurrealSchema>::nested_fields(
                #table_lit,
                #field_name_lit,
                    )
                );
            }
        });
        }
        else if surreal_type == "__dynamic_option__" {
            let nested_type: syn::Type = syn::parse_str(&extras[0]).unwrap();
            dynamic_nested.push(quote! {
                {
                    let resolved = <#nested_type as orm::model::SurrealType>::surreal_type();
                    parts.push(format!(
                        "DEFINE FIELD IF NOT EXISTS {} ON {} TYPE option<{}> {};\n",
                        #field_name_lit,
                        #table_lit,
                        resolved,
                        if resolved == "object" { "FLEXIBLE" } else { "" },
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
        }else {
            // known primitive type — static field def
            let is_flexible = surreal_type == "object" || surreal_type == "option<object>";
            let field_def = if is_flexible {
                format!("DEFINE FIELD IF NOT EXISTS {} ON {} TYPE {} FLEXIBLE;\n", field_name, table_name, surreal_type)
            } else {
                format!("DEFINE FIELD IF NOT EXISTS {} ON {} TYPE {};\n", field_name, table_name, surreal_type)
            };
            static_fields.push_str(&field_def);
        }

    // static_fields.push_str(&format!(
    //     "DEFINE FIELD IF NOT EXISTS {} ON {} TYPE {};\n",
    //     &field_name, &table_name, &surreal_type
    // ));

        // For each "object" field, emit a runtime call to SurrealSchema
        for nested_type_name in &extras {
            // extras contains the struct name when type is unknown
            let nested_type: syn::Type = syn::parse_str(nested_type_name).unwrap();
            dynamic_nested.push(quote! {
                parts.extend(
                    <#nested_type as orm::model::SurrealSchema>::nested_fields(
                        #table_lit,
                        #field_name_lit,
                    )
                );
            });
        }
    }
    let static_lit = syn::LitStr::new(&static_fields, span);
    // let migration_lit = syn::LitStr::new(&migration_sql, span);

    // ── relation attributes ──
    let relations = collect_relation_attrs(&input.attrs);

    let mut relation_methods = vec![];
    let mut registration_calls = vec![];

    for rel in &relations {
        match rel {
            RelationAttr::BelongsTo(parent_path) => {
                let parent_ident = &parent_path.segments.last().unwrap().ident;
                let snake = parent_ident.to_string().to_case(Case::Snake);
                let fn_name = Ident::new(&snake, span);
                let snake_lit = syn::LitStr::new(&snake, span);
                let fk_field = Ident::new(&snake, span);

                relation_methods.push(quote! {
                    pub fn #fn_name<'a,R>(
                        &self,
                        repo: &'a orm::repository::Repo,
                    ) -> impl std::future::Future<Output = Result<Option<R>, orm::error::ErrorIO>>
                    where
                        R: serde::de::DeserializeOwned + surrealdb::types::SurrealValue,
                    {
                        let rel: orm::model::BelongsTo<'a, #parent_path> =
                            orm::model::BelongsTo::new(repo, self.#fk_field.clone());
                        rel.one::<R>()
                    }
                });

                registration_calls.push(quote! {
                    orm::model::Relation::belongs_to(
                        #table_lit,
                        #snake_lit,
                        <#parent_path as orm::model::Model>::table_name(),
                    );
                });
            }

            RelationAttr::HasMany(child_path) => {
                let child_ident = &child_path.segments.last().unwrap().ident;
                let snake = child_ident.to_string().to_case(Case::Snake);
                let plural = pluralize(&snake, 2, false);
                let fn_name = Ident::new(&plural, span);
                let plural_lit = syn::LitStr::new(&plural, span);
                let parent_snake = struct_name.to_string().to_case(Case::Snake);
                let fk_field = format!("{}_id", parent_snake);

                relation_methods.push(quote! {
                    pub fn #fn_name<'a>(
                        &self,
                        repo: &'a orm::repository::Repo,
                    ) -> orm::model::HasMany<'a, #child_path> {
                        orm::model::HasMany::new(repo, #fk_field, self.id.clone())
                    }
                });

                registration_calls.push(quote! {
                    orm::model::Relation::has_many(
                        #table_lit,
                        #plural_lit,
                        <#child_path as orm::model::Model>::table_name(),
                    );
                });
            }

            RelationAttr::BelongsToMany { related, pivot, is_left } => {
                let fn_name = relation_plural_fn_name_from_path(related, span);
                let fn_name_lit = syn::LitStr::new(&fn_name.to_string(), span);
                let is_left_token = if *is_left { quote! { true } } else { quote! { false } };

                relation_methods.push(quote! {
                    pub fn #fn_name<'a>(
                        &self,
                        repo: &'a orm::repository::Repo,
                    ) -> orm::model::BelongsToMany<'a, #pivot, #related, Self> {
                        orm::model::BelongsToMany::new(repo, self.id.clone(), #is_left_token)
                    }
                });

                registration_calls.push(quote! {
                    orm::model::Relation::belongs_to_many(
                        #table_lit,
                        #fn_name_lit,
                        <#related as orm::model::Model>::table_name(),
                        <#pivot as orm::model::Model>::table_name(),
                        <#pivot as orm::model::Pivot>::left_key(),
                        <#pivot as orm::model::Pivot>::right_key(),
                        #is_left_token,
                    );
                });
            }
        }
    }

    let expanded = quote! {
        impl #struct_name {
            // ── CRUD ──
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
            pub fn delete<'a>(self, repo: &'a orm::repository::Repo)
                -> impl std::future::Future<Output = Result<usize, orm::error::ErrorIO>> + 'a
            {
                let query = orm::model::Query::<Self, orm::model::query::Delete>::new(repo);
                query.find(self.id.clone()).exec()
            }

            // ── Relation methods ──
            #(#relation_methods)*

            // ── Eager registration ──
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
        }
    };

    TokenStream::from(expanded)
}


// #[proc_macro_derive(SurrealTypeStruct)]
// pub fn derive_surreal_type_struct(input: TokenStream) -> TokenStream {
//     let input = parse_macro_input!(input as DeriveInput);
//     let struct_name = &input.ident;
//     quote! {
//         impl orm::model::SurrealType for #struct_name {
//             fn surreal_type() -> &'static str { "object" }
//         }
//     }.into()
// }
#[proc_macro_derive(SurrealTypeStruct)]
pub fn derive_surreal_type_struct(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    quote! {
        impl orm::model::SurrealType for #struct_name {
            fn surreal_type() -> &'static str { "object" }
        }
        // SurrealSchema is implemented separately via #[derive(SurrealSchema)]
    }.into()
}

// #[proc_macro_derive(SurrealTypeEnum)]
// pub fn derive_surreal_type_enum(input: TokenStream) -> TokenStream {
//     let input = parse_macro_input!(input as DeriveInput);
//     let struct_name = &input.ident;
//     quote! {
//         impl orm::model::SurrealType for #struct_name {
//             fn surreal_type() -> &'static str { "string" }
//         }
//     }.into()
// }
#[proc_macro_derive(SurrealTypeEnum)]
pub fn derive_surreal_type_enum(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    quote! {
        impl orm::model::SurrealType for #struct_name {
            fn surreal_type() -> &'static str { "string" }
        }
        // empty impl uses the default no-op nested_fields
        impl orm::model::SurrealSchema for #struct_name {}
    }.into()
}
#[proc_macro_derive(SurrealSchema)]
pub fn derive_surreal_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    // let table_name = to_snake_case(&struct_name.to_string());
    let fields = match &input.data {
        syn::Data::Struct(data) => &data.fields,
        _ => panic!("SurrealSchema only works on structs"),
    };
    
    let mut field_defs = vec![];
    for field in fields.iter() {
        let ident = field.ident.as_ref().unwrap();
        let field_name = ident.to_string();
        let (surreal_type, _) = rust_type_to_surreal(
            &field_name, &field.ty, "", &field_name
        );

        let is_flexible = surreal_type == "object" 
            || surreal_type == "option<object>";

        if is_flexible {
            field_defs.push(quote! {
                defs.push(format!(
                    "DEFINE FIELD IF NOT EXISTS {}.{} ON {} FLEXIBLE TYPE {};\n",
                    prefix, #field_name, table, #surreal_type
                ));
            });
        } else {
            field_defs.push(quote! {
                defs.push(format!(
                    "DEFINE FIELD IF NOT EXISTS {}.{} ON {} TYPE {};\n",
                    prefix, #field_name, table, #surreal_type
                ));
            });
        }
    }

    quote! {
        impl orm::model::SurrealSchema for #struct_name {
            fn nested_fields(table: &str, prefix: &str) -> Vec<String> {
                let mut defs = Vec::new();
                #(#field_defs)*
                defs
            }
        }
    }.into()
}

// ─────────────────────────────────────────────
// RegisterRelations — standalone derive (kept for backward compat)
// ─────────────────────────────────────────────


#[proc_macro_derive(PivotModel, attributes(left, right, timestamp))]
pub fn pivot_model_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = input.ident.clone();
    let mut table_name = struct_name.to_string().to_case(Case::Snake);
    // 1️⃣ Collect fields
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => named.named.iter().collect::<Vec<_>>(),
            _ => panic!("PivotModel requires named fields"),
        },
        _ => panic!("PivotModel can only be derived for structs"),
    };

    // 2️⃣ Detect left and right
    let mut left_field = None;
    let mut right_field = None;

    for field in &fields {
        for attr in &field.attrs {
            if attr.path().is_ident("left") {
                left_field = Some(field.ident.clone().unwrap());
            }
            if attr.path().is_ident("right") {
                right_field = Some(field.ident.clone().unwrap());
            }
        }
    }

    let left_ident = left_field.expect("Missing #[left] field");
    let right_ident = right_field.expect("Missing #[right] field");

    // 3️⃣ Detect timestamp attribute
    let has_timestamp = input.attrs.iter().any(|attr| attr.path().is_ident("timestamp"));

    // 4️⃣ Collect extra fields (exclude id, left, right, timestamps)
    let extra_fields: Vec<(syn::Ident, &syn::Type)> = fields
        .iter()
        .filter_map(|f| {
            let ident = f.ident.clone().unwrap();
            let ty = &f.ty;
            if ident != left_ident
                && ident != right_ident
                && ident != "id"
                && !(has_timestamp && (ident == "created_at" || ident == "updated_at"))
            {
                Some((ident, ty))
            } else {
                None
            }
        })
        .collect();

    // 5️⃣ Generate Extra tuple type
    let extra_type = if extra_fields.is_empty() {
        quote! { () }
    } else {
        let types = extra_fields.iter().map(|(_, ty)| quote! { #ty });
        quote! { ( #( #types ),* ) }
    };

    // 6️⃣ Generate tuple destructuring
    let extra_destructure = if extra_fields.is_empty() {
        quote! {}
    } else {
        let idents = extra_fields.iter().map(|(ident, _)| ident);
        quote! {
            let ( #( #idents ),* ) = extra;
        }
    };

    // 7️⃣ Generate assignments for Self { ... }
    let extra_assignments = extra_fields.iter().map(|(ident, _)| quote! { #ident: #ident });

    // 8️⃣ Timestamp initialization
    let timestamp_init = if has_timestamp {
        quote! { let now = chrono::Utc::now().to_rfc3339(); }
    } else {
        quote! {}
    };

    let timestamp_assign = if has_timestamp {
        quote! {
            created_at: now.clone(),
            updated_at: now,
        }
    } else {
        quote! {}
    };
    
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

let mut migration_sql = format!(
        "DEFINE TABLE IF NOT EXISTS {} SCHEMAFULL;\n",
        table_name
    );

    for field in &fields {
        let ident = field.ident.as_ref().unwrap();

        if ident == "id" {
            continue;
        }

        let field_name = ident.to_string();

        let (surreal_type, _extras) =
            rust_type_to_surreal(&field_name, &field.ty, &table_name, &field_name);

        migration_sql.push_str(
            &format!(
                "DEFINE FIELD IF NOT EXISTS {} ON {} TYPE {};\n",
                field_name,
                table_name,
                surreal_type
            )
        );
    }

    let migration_literal =
        syn::LitStr::new(
            &migration_sql,
            proc_macro2::Span::call_site()
        );

    let table_literal =
        syn::LitStr::new(
            &table_name,
            proc_macro2::Span::call_site()
        );

    let expanded = quote! {

        impl orm::model::Model for #struct_name {
            fn table_name() -> &'static str {
                #table_literal
            }

            fn id(&self) -> surrealdb::types::RecordId {
                self.id.clone()
            }
            fn schema() -> String {
                #migration_literal.to_string()
            }
        }

        impl orm::model::Pivot for #struct_name {

            type Extra = #extra_type;

            fn left_key() -> &'static str {
                stringify!(#left_ident)
            }

            fn right_key() -> &'static str {
                stringify!(#right_ident)
            }

            fn left_id(&self) -> surrealdb::types::RecordId {
                self.#left_ident.clone()
            }

            fn right_id(&self) -> surrealdb::types::RecordId {
                self.#right_ident.clone()
            }

            /// Constructor with extra fields
            fn new_with(left: surrealdb::types::RecordId, right: surrealdb::types::RecordId, extra: Self::Extra) -> Self {
                #extra_destructure
                #timestamp_init

                Self {
                    id: surrealdb::types::RecordId{
                        table: Self::table_name().into(),
                        key: surrealdb::types::RecordIdKey::String(surrealdb::types::Uuid::new_v4().to_string())
                    },
                    #left_ident: left,
                    #right_ident: right,
                    #( #extra_assignments, )*
                    #timestamp_assign
                }
            }

            /// Convenience constructor for builder/old code
            fn new(left: surrealdb::types::RecordId, right: surrealdb::types::RecordId) -> Self {
                Self::new_with(left, right, Default::default())
            }
        }
    };

    TokenStream::from(expanded)
}



// use proc_macro::TokenStream;
// use quote::{quote};
// use syn::{parse_macro_input, ItemStruct, DeriveInput, Meta, Lit, punctuated::Punctuated, Token, Path};
// use syn::{
//     parse::{Parse, ParseStream},
//     Ident
// };
// use syn::{
//     Type, PathArguments, GenericArgument,
// };
// use syn::parse::Parser;

// use convert_case::{Case, Casing};
// use pluralizer::pluralize;

// use syn::{Data, Fields};

// #[proc_macro_derive(AllowedRelations, attributes(belongs_to, belongs_to_many, has_many))]
// pub fn derive_allowed_relations(input: TokenStream) -> TokenStream {
//     let input = parse_macro_input!(input as DeriveInput);
//     let struct_name = &input.ident;
//     let relation_enum_name = syn::Ident::new(
//         &format!("{}Relation", struct_name),
//         struct_name.span(),
//     );

//     let mut variants = vec![];
//     let mut from_str_arms = vec![];
//     let mut as_str_arms = vec![];

//     for attr in &input.attrs {
//         let meta_name = attr.path().get_ident().map(|i| i.to_string());

//         match meta_name.as_deref() {
//             Some("belongs_to") | Some("has_many") => {
//                 if let Ok(syn::Meta::List(list)) = attr.meta.clone().try_into() {
//                     if let Ok(ty) = syn::parse2::<syn::Path>(list.tokens.clone()) {
//                         let type_name = ty.segments.last().unwrap().ident.to_string();
//                         let snake = to_snake_case(&type_name);

//                         let variant = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
//                         let snake_lit = syn::LitStr::new(&snake, proc_macro2::Span::call_site());

//                         variants.push(quote! { #variant });
//                         from_str_arms.push(quote! { #snake_lit => Some(Self::#variant) });
//                         as_str_arms.push(quote! { Self::#variant => #snake_lit });
//                     }
//                 }
//             }
//             Some("belongs_to_many") => {
//                 if let Ok(syn::Meta::List(list)) = attr.meta.clone().try_into() {
//                     let args = syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated
//                         .parse2(list.tokens.clone())
//                         .unwrap();
//                     if let Some(first) = args.first() {
//                         let type_name = first.segments.last().unwrap().ident.to_string();
//                         let snake = to_snake_case(&type_name);

//                         let variant = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
//                         let snake_lit = syn::LitStr::new(&snake, proc_macro2::Span::call_site());

//                         variants.push(quote! { #variant });
//                         from_str_arms.push(quote! { #snake_lit => Some(Self::#variant) });
//                         as_str_arms.push(quote! { Self::#variant => #snake_lit });
//                     }
//                 }
//             }
//             _ => {}
//         }
//     }

//     let expanded = quote! {
//         #[derive(Debug, Clone, PartialEq, Eq)]
//         pub enum #relation_enum_name {
//             #(#variants),*
//         }

//         impl #relation_enum_name {
//             pub fn from_str(s: &str) -> Option<Self> {
//                 match s {
//                     #(#from_str_arms,)*
//                     _ => None,
//                 }
//             }

//             pub fn as_str(&self) -> &'static str {
//                 match self {
//                     #(#as_str_arms,)*
//                 }
//             }
//         }
//     };

//     TokenStream::from(expanded)
// }

// fn to_snake_case(s: &str) -> String {
//     let mut result = String::new();
//     for (i, ch) in s.chars().enumerate() {
//         if ch.is_uppercase() && i > 0 {
//             result.push('_');
//         }
//         result.push(ch.to_lowercase().next().unwrap());
//     }
//     result
// }


// fn rust_type_to_surreal(field_name: &str, ty: &Type) -> String {
//     match ty {
//         Type::Path(type_path) => {
//             let segment =
//                 &type_path.path.segments.last().unwrap();

//             let ident =
//                 segment.ident.to_string();

//             match ident.as_str() {
//                 "String" => "string".to_string(),
//                 "bool" => "bool".to_string(),

//                 "i8" | "i16" | "i32" | "i64" |
//                 "u8" | "u16" | "u32" | "u64" |
//                 "usize" | "isize" => "int".to_string(),

//                 "f32" | "f64" => "float".to_string(),

//                 "RecordId" => {
//                     if field_name == "id" {
//                         "record".to_string()
//                     } else {
//                         format!("record<{}>", field_name)
//                     }
//                 },

//                 "Option" => {
//                     if let PathArguments::AngleBracketed(args) =
//                         &segment.arguments
//                     {
//                         if let Some(GenericArgument::Type(inner_ty)) =
//                             args.args.first()
//                         {
//                             let inner =
//                                 rust_type_to_surreal(field_name, inner_ty);

//                             return format!(
//                                 "option<{}>",
//                                 inner
//                             );
//                         }
//                     }

//                     "option<any>".to_string()
//                 }

//                 "Vec" => {
//                     if let PathArguments::AngleBracketed(args) =
//                         &segment.arguments
//                     {
//                         if let Some(GenericArgument::Type(inner_ty)) =
//                             args.args.first()
//                         {
//                             let inner =
//                                 rust_type_to_surreal(field_name, inner_ty);

//                             return format!(
//                                 "array<{}>",
//                                 inner
//                             );
//                         }
//                     }

//                     "array".to_string()
//                 }

//                 _ => "any".to_string(),
//             }
//         }

//         _ => "any".to_string(),
//     }
// }


// /// Helper to generate plural snake_case method names
// fn relation_plural_fn_name_from_path(path: &Path, span: proc_macro2::Span) -> Ident {
//     let segment = path
//         .segments
//         .last()
//         .expect("Expected a valid type path for relation");

//     let snake = segment.ident.to_string().to_case(Case::Snake);
//     let plural = pluralize(&snake, 2, false);

//     Ident::new(&plural, span)
// }

// #[proc_macro_derive(Model, attributes(table))]
// pub fn derive_model(input: TokenStream) -> TokenStream {
//     let input = parse_macro_input!(input as DeriveInput);

//     let struct_name = input.ident;

//     let mut table_name = to_snake_case(&struct_name.to_string());

//     let fields = match input.data {
//         syn::Data::Struct(data) => data.fields,
//         _ => panic!("Model can only be derived for structs"),
//     };

//     fields.iter().find(|f| {
//         f.ident.as_ref().unwrap() == "id"
//     }).expect("Model requires an `id` field");

//     for attr in input.attrs {
//         if attr.path().is_ident("table") {
//             if let Meta::NameValue(meta) = attr.meta {
//                 if let syn::Expr::Lit(expr_lit) = meta.value {
//                     if let Lit::Str(lit_str) = expr_lit.lit {
//                         table_name = lit_str.value();
//                     }
//                 }
//             }
//         }
//     }

//     let mut migration_sql = format!(
//         "DEFINE TABLE IF NOT EXISTS {} SCHEMAFULL;\n",
//         table_name
//     );

//     for field in fields.iter() {
//         let ident = field.ident.as_ref().unwrap();

//         if ident == "id" {
//             continue;
//         }

//         let field_name = ident.to_string();

//         let surreal_type =
//             rust_type_to_surreal(&field_name, &field.ty);

//         migration_sql.push_str(
//             &format!(
//                 "DEFINE FIELD IF NOT EXISTS {} ON {} TYPE {};\n",
//                 field_name,
//                 table_name,
//                 surreal_type
//             )
//         );
//     }

//     let expanded = quote! {
//         impl #struct_name {
//             pub fn insert<'a>(repo: &'a orm::repository::Repo) -> orm::model::Query<'a, Self, orm::model::query::Insert> {
//                 orm::model::Query::<Self, orm::model::query::Insert>::new(repo)
//             }

//             pub fn select<'a>(repo: &'a orm::repository::Repo) -> orm::model::Query<'a, Self, orm::model::query::Select> {
//                 orm::model::Query::<Self, orm::model::query::Select>::new(repo)
//             }

//             pub fn update<'a>(repo: &'a orm::repository::Repo) -> orm::model::Query<'a, Self, orm::model::query::Update> {
//                 orm::model::Query::<Self, orm::model::query::Update>::new(repo)
//             }

//             pub fn destroy<'a>(repo: &'a orm::repository::Repo) -> orm::model::Query<'a, Self, orm::model::query::Delete> {
//                 orm::model::Query::<Self, orm::model::query::Delete>::new(repo)
//             }

//             pub fn save<'a>(self, repo: &'a orm::repository::Repo) -> impl std::future::Future<Output = Result<Self, orm::error::ErrorIO>> {
//                 let query = orm::model::Query::<Self, orm::model::query::Update>::new(repo);
//                 query.find(self.id.clone()).values(self).exec::<Self>()
//             }

//             pub fn delete<'a>(self, repo: &'a orm::repository::Repo) -> impl std::future::Future<Output = Result<usize, orm::error::ErrorIO>> {
//                 let query = orm::model::Query::<Self, orm::model::query::Delete>::new(repo);
//                 query.find(self.id.clone()).exec()
//             }
//         }

//         impl orm::model::Model for #struct_name {
//             fn table_name() -> &'static str {
//                 #table_name
//             }

//             fn id(&self) -> surrealdb::types::RecordId {
//                 self.id.clone()
//             }

//             fn schema() -> &'static str {
//                 #migration_sql
//             }
//         }
//     };

//     TokenStream::from(expanded)
// }




// #[proc_macro_attribute]
// pub fn belongs_to(attr: TokenStream, item: TokenStream) -> TokenStream {
//     let input = parse_macro_input!(item as syn::ItemStruct);
//     let parent_name = parse_macro_input!(attr as syn::Path);

//     let struct_name = &input.ident;

//     let parent_segment = parent_name
//         .segments
//         .last()
//         .expect("Expected a valid parent type path");

//     let parent_ident = &parent_segment.ident;

//     use convert_case::{Case, Casing};

//     let snake = parent_ident.to_string().to_case(Case::Snake);
//     let snake_lit = syn::LitStr::new(&snake, parent_ident.span());

//     let fn_name = syn::Ident::new(&snake, struct_name.span());

//     let fk_field = syn::Ident::new(&snake, struct_name.span());

//     let expanded = quote! {
//         #input

//         impl #struct_name {
//             pub fn #fn_name<'a,R>(
//                 &self,
//                 repo: &'a orm::repository::Repo
//             ) -> impl Future<Output = Result<Option<R>, orm::error::ErrorIO>>
//             where R: serde::de::DeserializeOwned +  surrealdb::types::SurrealValue
//             {
//                 {
//                     use std::sync::Once;
//                     static INIT: Once = Once::new();
//                     INIT.call_once(|| {
//                         orm::model::Relation::belongs_to(
//                             <Self as orm::model::Model>::table_name(),
//                             #snake_lit,
//                             <#parent_ident as orm::model::Model>::table_name(),
//                         );
//                     });
//                 }
//                 let rel: orm::model::BelongsTo<'a, #parent_ident> = orm::model::BelongsTo::new(&repo,self.#fk_field.clone());
//                 rel.one::<R>()
//             }
//         }
//     };

//     TokenStream::from(expanded)
// }


// #[proc_macro_attribute]
// pub fn has_many(attr: TokenStream, item: TokenStream) -> TokenStream {
//     let input = parse_macro_input!(item as syn::ItemStruct);
//     let child_path = parse_macro_input!(attr as syn::Path);

//     let struct_name = &input.ident;

//     let child_segment = child_path
//         .segments
//         .last()
//         .expect("Expected a valid child type path");

//     use convert_case::{Case, Casing};
//     use pluralizer::pluralize;

//     let snake = child_segment.ident.to_string().to_case(Case::Snake);

//     let plural = pluralize(&snake, 2, false);

//     let fn_name = syn::Ident::new(&plural, struct_name.span());
//     let plural_lit = syn::LitStr::new(&plural, struct_name.span());

//     let parent_snake = struct_name.to_string().to_case(Case::Snake);
//     let fk_field = format!("{}_id", parent_snake);

//     let expanded = quote! {
//         #input

//         impl #struct_name {
//             pub fn #fn_name<'a>(
//                 &self,
//                 repo: &'a orm::repository::Repo
//             ) -> orm::model::HasMany<'a, #child_path> {
//                 {
//                     use std::sync::Once;
//                     static INIT: Once = Once::new();
//                     INIT.call_once(|| {
//                         orm::model::Relation::has_many(
//                             <Self as orm::model::Model>::table_name(),
//                             #plural_lit,
//                             <#child_path as orm::model::Model>::table_name(),
//                         );
//                     });
//                 }
//                 orm::model::HasMany::new(
//                     repo,
//                     #fk_field,
//                     self.id.clone(),
//                 )
//             }
//         }
//     };

//     TokenStream::from(expanded)
// }



// struct BelongsToManyArgs {
//     related: Path,
//     pivot: Path,
//     side: Ident,
// }

// impl Parse for BelongsToManyArgs {
//     fn parse(input: ParseStream) -> syn::Result<Self> {
//         let args: Punctuated<syn::Expr, Token![,]> =
//             Punctuated::parse_terminated(input)?;

//         if args.len() != 3 {
//             return Err(input.error(
//                 "Expected: RelatedModel, PivotModel, left|right"
//             ));
//         }

//         let mut iter = args.into_iter();

//         let related = match iter.next().unwrap() {
//             syn::Expr::Path(p) => p.path,
//             _ => return Err(input.error("Expected model type")),
//         };

//         let pivot = match iter.next().unwrap() {
//             syn::Expr::Path(p) => p.path,
//             _ => return Err(input.error("Expected pivot type")),
//         };

//         let side = match iter.next().unwrap() {
//             syn::Expr::Path(p) => {
//                 p.path.get_ident()
//                     .cloned()
//                     .ok_or_else(|| input.error("Expected left or right"))?
//             }
//             _ => return Err(input.error("Expected left or right")),
//         };

//         if side != "left" && side != "right" {
//             return Err(input.error("Side must be `left` or `right`"));
//         }

//         Ok(Self { related, pivot, side })
//     }
// }

// #[proc_macro_attribute]
// pub fn belongs_to_many(attr: TokenStream, item: TokenStream) -> TokenStream {
//     let input = parse_macro_input!(item as ItemStruct);
//     let args = parse_macro_input!(attr as BelongsToManyArgs);

//     let struct_name = &input.ident;
//     let related_path = args.related;
//     let pivot_path = args.pivot;
//     let side = args.side;
//     let fn_name =
//         relation_plural_fn_name_from_path(&related_path, struct_name.span());
//     let fn_name_str = fn_name.to_string();
//     let fn_name_lit = syn::LitStr::new(&fn_name_str, struct_name.span());

//     let is_left = if side == "left" {
//         quote! { true }
//     } else {
//         quote! { false }
//     };

//     let expanded = quote! {
//         #input

//         impl #struct_name
//         where
//             Self: orm::model::Model,
//         {
//             pub fn #fn_name<'a>(
//                 &self,
//                 repo: &'a orm::repository::Repo,
//             ) -> orm::model::BelongsToMany<'a, #pivot_path, #related_path, #struct_name>
//             {
//                 {
//                     use std::sync::Once;
//                     static INIT: Once = Once::new();
//                     INIT.call_once(|| {
//                         orm::model::Relation::belongs_to_many(
//                             <Self as orm::model::Model>::table_name(),
//                             #fn_name_lit,
//                             <#related_path as orm::model::Model>::table_name(),
//                             <#pivot_path as orm::model::Model>::table_name(),
//                             <#pivot_path as orm::model::Pivot>::left_key(),
//                             <#pivot_path as orm::model::Pivot>::right_key(),
//                             #is_left,
//                         );
//                     });
//                 }
//                 orm::model::BelongsToMany::new(
//                     repo,
//                     self.id.clone(),
//                     #is_left
//                 )
//             }
//         }
//     };

//     TokenStream::from(expanded)
// }

// fn rust_type_to_surreal(field_name: &str, ty: &Type) -> String {
//     match ty {
//         Type::Path(type_path) => {
//             let segment = &type_path.path.segments.last().unwrap();
//             let ident = segment.ident.to_string();
//             match ident.as_str() {
//                 "String" => "string".to_string(),
//                 "bool" => "bool".to_string(),
//                 "i8" | "i16" | "i32" | "i64" |
//                 "u8" | "u16" | "u32" | "u64" |
//                 "usize" | "isize" => "int".to_string(),
//                 "f32" | "f64" => "float".to_string(),
//                 "RecordId" => {
//                     if field_name == "id" { "record".to_string() }
//                     else { format!("record<{}>", field_name) }
//                 }
//                 "Option" => {
//                     if let PathArguments::AngleBracketed(args) = &segment.arguments {
//                         if let Some(GenericArgument::Type(inner)) = args.args.first() {
//                             return format!("option<{}>", rust_type_to_surreal(field_name, inner));
//                         }
//                     }
//                     "option<any>".to_string()
//                 }
//                 "Vec" => {
//                     if let PathArguments::AngleBracketed(args) = &segment.arguments {
//                         if let Some(GenericArgument::Type(inner)) = args.args.first() {
//                             return format!("array<{}>", rust_type_to_surreal(field_name, inner));
//                         }
//                     }
//                     "array".to_string()
//                 }
//                 _ => "any".to_string(),
//             }
//         }
//         _ => "any".to_string(),
//     }
// }
// #[proc_macro_derive(RegisterRelations, attributes(belongs_to, belongs_to_many, has_many, table))]
// pub fn derive_register_relations(input: TokenStream) -> TokenStream {
//     let input = parse_macro_input!(input as DeriveInput);
//     let struct_name = &input.ident;
//     let span = struct_name.span();

//     let mut table_name = to_snake_case(&struct_name.to_string());
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
//     let table_lit = syn::LitStr::new(&table_name, span);

//     let relations = collect_relation_attrs(&input.attrs);
//     let mut registration_calls = vec![];

//     for rel in &relations {
//         match rel {
//             RelationAttr::BelongsTo(parent_path) => {
//                 let parent_ident = &parent_path.segments.last().unwrap().ident;
//                 let snake = parent_ident.to_string().to_case(Case::Snake);
//                 let snake_lit = syn::LitStr::new(&snake, span);
//                 registration_calls.push(quote! {
//                     orm::model::Relation::belongs_to(
//                         #table_lit, #snake_lit,
//                         <#parent_path as orm::model::Model>::table_name(),
//                     );
//                 });
//             }
//             RelationAttr::HasMany(child_path) => {
//                 let child_ident = &child_path.segments.last().unwrap().ident;
//                 let snake = child_ident.to_string().to_case(Case::Snake);
//                 let plural = pluralize(&snake, 2, false);
//                 let plural_lit = syn::LitStr::new(&plural, span);
//                 registration_calls.push(quote! {
//                     orm::model::Relation::has_many(
//                         #table_lit, #plural_lit,
//                         <#child_path as orm::model::Model>::table_name(),
//                     );
//                 });
//             }
//             RelationAttr::BelongsToMany { related, pivot, is_left } => {
//                 let fn_name = relation_plural_fn_name_from_path(related, span);
//                 let fn_name_lit = syn::LitStr::new(&fn_name.to_string(), span);
//                 let is_left_token = if *is_left { quote! { true } } else { quote! { false } };
//                 registration_calls.push(quote! {
//                     orm::model::Relation::belongs_to_many(
//                         #table_lit, #fn_name_lit,
//                         <#related as orm::model::Model>::table_name(),
//                         <#pivot as orm::model::Model>::table_name(),
//                         <#pivot as orm::model::Pivot>::left_key(),
//                         <#pivot as orm::model::Pivot>::right_key(),
//                         #is_left_token,
//                     );
//                 });
//             }
//         }
//     }

//     let expanded = quote! {
//         impl #struct_name {
//             pub fn register_relations() {
//                 #(#registration_calls)*
//             }
//         }
//     };

//     TokenStream::from(expanded)
// }



// #[proc_macro_derive(RegisterRelations, attributes(belongs_to, belongs_to_many, has_many, table))]
// pub fn derive_register_relations(input: TokenStream) -> TokenStream {
//     let input = parse_macro_input!(input as DeriveInput);
//     let struct_name = &input.ident;

//     // Resolve table name
//     let mut table_name = to_snake_case(&struct_name.to_string());
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

//     let mut registration_calls = vec![];

//     for attr in &input.attrs {
//         let meta_name = attr.path().get_ident().map(|i| i.to_string());

//         match meta_name.as_deref() {
//             Some("belongs_to") => {
//                 if let Ok(syn::Meta::List(list)) = attr.meta.clone().try_into() {
//                     if let Ok(parent_path) = syn::parse2::<syn::Path>(list.tokens.clone()) {
//                         let parent_ident = &parent_path.segments.last().unwrap().ident;
//                         let snake = parent_ident.to_string().to_case(Case::Snake);
//                         let snake_lit = syn::LitStr::new(&snake, parent_ident.span());
//                         let table_lit = syn::LitStr::new(&table_name, struct_name.span());

//                         registration_calls.push(quote! {
//                             orm::model::Relation::belongs_to(
//                                 #table_lit,
//                                 #snake_lit,
//                                 <#parent_path as orm::model::Model>::table_name(),
//                             );
//                         });
//                     }
//                 }
//             }

//             Some("has_many") => {
//                 if let Ok(syn::Meta::List(list)) = attr.meta.clone().try_into() {
//                     if let Ok(child_path) = syn::parse2::<syn::Path>(list.tokens.clone()) {
//                         let child_ident = &child_path.segments.last().unwrap().ident;
//                         let snake = child_ident.to_string().to_case(Case::Snake);
//                         let plural = pluralize(&snake, 2, false);
//                         let plural_lit = syn::LitStr::new(&plural, child_ident.span());
//                         let table_lit = syn::LitStr::new(&table_name, struct_name.span());

//                         registration_calls.push(quote! {
//                             orm::model::Relation::has_many(
//                                 #table_lit,
//                                 #plural_lit,
//                                 <#child_path as orm::model::Model>::table_name(),
//                             );
//                         });
//                     }
//                 }
//             }

//             Some("belongs_to_many") => {
//                 if let Ok(syn::Meta::List(list)) = attr.meta.clone().try_into() {
//                     // Parse: RelatedModel, PivotModel, left|right
//                     let args = syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated
//                         .parse2(list.tokens.clone());

//                     if let Ok(args) = args {
//                         let mut iter = args.iter();
//                         let related = iter.next();
//                         let pivot = iter.next();
//                         let side = iter.next();

//                         if let (Some(related_path), Some(pivot_path), Some(side_path)) =
//                             (related, pivot, side)
//                         {
//                             let related_ident = &related_path.segments.last().unwrap().ident;
//                             let snake = related_ident.to_string().to_case(Case::Snake);
//                             let plural = pluralize(&snake, 2, false);
//                             let plural_lit = syn::LitStr::new(&plural, related_ident.span());
//                             let table_lit = syn::LitStr::new(&table_name, struct_name.span());

//                             let side_str = side_path
//                                 .get_ident()
//                                 .map(|i| i.to_string())
//                                 .unwrap_or_default();
//                             let is_left = side_str == "left";
//                             let is_left_token = if is_left {
//                                 quote! { true }
//                             } else {
//                                 quote! { false }
//                             };

//                             registration_calls.push(quote! {
//                                 orm::model::Relation::belongs_to_many(
//                                     #table_lit,
//                                     #plural_lit,
//                                     <#related_path as orm::model::Model>::table_name(),
//                                     <#pivot_path as orm::model::Model>::table_name(),
//                                     <#pivot_path as orm::model::Pivot>::left_key(),
//                                     <#pivot_path as orm::model::Pivot>::right_key(),
//                                     #is_left_token,
//                                 );
//                             });
//                         }
//                     }
//                 }
//             }

//             _ => {}
//         }
//     }

//     let expanded = quote! {
//         impl #struct_name {
//             pub fn register_relations() {
//                 #(#registration_calls)*
//             }
//         }
//     };

//     TokenStream::from(expanded)
// }