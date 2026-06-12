use std::collections::HashMap;

use crate::{model::relations::{Relation, RelationType}, pluralizer};

pub fn do_magic(param: &str) -> HashMap<String, Option<Vec<String>>> {
    let rel: String = param.chars().filter(|c| !c.is_whitespace()).collect();
    let mut all = HashMap::new();
    for r in rel.split(',').filter(|s| !s.is_empty()) {
        let temp: Vec<&str> = r.split('.').filter(|s| !s.is_empty()).collect();
        all.insert(
            temp[0].to_string(),
            (temp.len() > 1).then(|| temp[1..].iter().map(|s| s.to_string()).collect()),
        );
    }
    all
}

#[derive(Debug, Clone)]
pub struct NestedRelation {
    pub name: String,
    pub children: Vec<NestedRelation>,
}

impl NestedRelation {
pub fn merge_into(nodes: &mut Vec<NestedRelation>, parts: &[&str]) {
        if parts.is_empty() {
            return;
        }
        let head = parts[0];
        let tail = &parts[1..];

        if let Some(existing) = nodes.iter_mut().find(|n| n.name == head) {
            Self::merge_into(&mut existing.children, tail);
        } else {
            let mut node = NestedRelation { name: head.to_string(), children: vec![] };
            let mut current = &mut node;
            for part in tail {
                current.children.push(NestedRelation {
                    name: part.to_string(),
                    children: vec![],
                });
                current = current.children.last_mut().unwrap();
            }
            nodes.push(node);
        }
    }

    pub fn parse_path(path: &str) -> Vec<NestedRelation> {
        let mut roots: Vec<NestedRelation> = vec![];
        for segment in path.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let parts: Vec<&str> = segment.split('.').filter(|s| !s.is_empty()).collect();
            Self::merge_into(&mut roots, &parts);
        }
        roots
    }
    pub fn to_subquery_sql(&self, parent_table: &str, parent_ref: &str) -> String {
        let lookup_name = self.name.to_lowercase();
        // Singularize once — used as fallback table name when registry has no child_table
        let singular = pluralizer::singularize(&lookup_name);

        // ── Registry lookup ───────────────────────────────────────────────────
        if let Some(rel) = Relation::get(parent_table, &lookup_name) {
            match rel.relation_type {

                // ── BelongsToMany ─────────────────────────────────────────────
                RelationType::BelongsToMany => {
                    // All three pivot fields must be present.
                    // If any is missing the relation was registered incorrectly —
                    // emit NULL and warn rather than panic or produce broken SQL.
                    let (pivot_table, pivot_left, pivot_right) = match (
                        rel.pivot.as_deref(),
                        rel.pivot_left_key.as_deref(),
                        rel.pivot_right_key.as_deref(),
                    ) {
                        (Some(p), Some(l), Some(r)) => (p, l, r),
                        _ => {
                            eprintln!(
                                "[orm] WARNING: BelongsToMany `{}` on `{}` is missing \
                                 pivot/pivot_left_key/pivot_right_key — skipping. \
                                 Check your #[belongs_to_many(...)] annotation.",
                                self.name, parent_table
                            );
                            return format!("NULL AS {}", self.name);
                        }
                    };

                    let (pivot_fk, pivot_child_key) = if rel.is_left.unwrap_or(true) {
                        (pivot_left, pivot_right)
                    } else {
                        (pivot_right, pivot_left)
                    };

                    let child_table = rel.child_table.as_deref().unwrap_or(&singular);

                    if self.children.is_empty() {
                        return format!(
                            "(SELECT * OMIT {pivot_fk} FROM {pivot_table} \
                             WHERE {pivot_fk} = {parent_ref}.id \
                             FETCH {pivot_child_key}) AS {}",
                            self.name
                        );
                    }

                    // Children → correlated subquery per pivot row
                    let mut inner = String::from("SELECT *");
                    for child in &self.children {
                        inner.push_str(&format!(
                            " , {}",
                            child.to_subquery_sql(child_table, "$parent")
                        ));
                    }
                    format!(
                        "(SELECT VALUE ({inner} FROM {child_table} \
                         WHERE id = $parent.{pivot_child_key})[0] \
                         FROM {pivot_table} \
                         WHERE {pivot_fk} = {parent_ref}.id) AS {}",
                        self.name
                    )
                }

                // ── HasMany ───────────────────────────────────────────────────
                RelationType::HasMany => {
                    let child_table = rel.child_table.as_deref().unwrap_or(&singular);

                    // FK column on the child side.
                    // rel.fk is populated by has_many_with_fk (called by the macro).
                    // Falls back to parent_table name (old SurrealDB convention) with a warning.
                    let fk_col = rel.fk.as_deref().unwrap_or_else(|| {
                        eprintln!(
                            "[orm] WARNING: HasMany `{}` on `{}` has no FK stored in the \
                             registry. Falling back to parent table name `{}` as the FK \
                             column. Fix with: #[has_many({}, fk = \"<col_name>\")]",
                            self.name, parent_table, parent_table, child_table,
                        );
                        parent_table
                    });

                    let mut inner = String::from("SELECT *");
                    for child in &self.children {
                        inner.push_str(&format!(
                            " , {}",
                            child.to_subquery_sql(child_table, "$parent")
                        ));
                    }
                    format!(
                        "({inner} FROM {child_table} \
                         WHERE {fk_col} = {parent_ref}.id) AS {}",
                        self.name
                    )
                }

                // ── BelongsTo ─────────────────────────────────────────────────
                RelationType::BelongsTo => {
                    let related_table = rel.child_table.as_deref().unwrap_or(&singular);

                    // FK column on the *current* row pointing to the related record.
                    //
                    // CRITICAL: self.name is the RELATION name — NOT the FK field.
                    // They differ when `to = "..."` is used, e.g.:
                    //   #[belongs_to(Family, fk = "parent", to = "parent_family")]
                    //   relation name = "parent_family", FK field = "parent"
                    //
                    // rel.fk is populated by belongs_to_with_fk (called by the macro).
                    // Falls back to relation name with a warning (works only when
                    // the relation name happens to equal the FK field name).
                    let fk_col = rel.fk.as_deref().unwrap_or_else(|| {
                        eprintln!(
                            "[orm] WARNING: BelongsTo `{}` on `{}` has no FK stored in the \
                             registry. Falling back to relation name `{}` as the FK column. \
                             This is WRONG if you used `to = \"...\"`. \
                             Fix with: #[belongs_to({}, fk = \"<col_name>\")]",
                            self.name, parent_table, self.name, related_table,
                        );
                        &self.name
                    });

                    let mut inner = String::from("SELECT *");
                    for child in &self.children {
                        inner.push_str(&format!(
                            " , {}",
                            child.to_subquery_sql(related_table, "$parent")
                        ));
                    }
                    format!(
                        "({inner} FROM {related_table} \
                         WHERE id = {parent_ref}.{fk_col})[0] AS {}",
                        self.name
                    )
                }
            }

        } else {
            // ── Fallback: relation not in registry ────────────────────────────
            //
            // Heuristic based on plurality:
            //   plural   → HasMany  (child has FK pointing to us)
            //   singular → BelongsTo (we have a FK field pointing to it)
            //
            // Always warns — this should never be relied on in production.
            eprintln!(
                "[orm] WARNING: relation `{}` not registered on table `{}`. \
                 Using heuristic fallback SQL. Add #[has_many] / #[belongs_to] / \
                 #[belongs_to_many] and call register_relations() to silence this.",
                self.name, parent_table
            );

            let mut inner = String::from("SELECT *");
            for child in &self.children {
                inner.push_str(&format!(
                    " , {}",
                    child.to_subquery_sql(&singular, "$parent")
                ));
            }

            if pluralizer::is_plural(&lookup_name) {
                // HasMany heuristic — FK column assumed to equal parent_table name
                format!(
                    "({inner} FROM {singular} \
                     WHERE {parent_table} = {parent_ref}.id) AS {}",
                    self.name
                )
            } else {
                // BelongsTo heuristic — FK column assumed to equal relation name
                format!(
                    "({inner} FROM {singular} \
                     WHERE id = {parent_ref}.{lookup_name})[0] AS {}",
                    self.name
                )
            }
        }
    }

    pub fn generate_nested_sql(nested: &[NestedRelation], root_parent: &str) -> String {
        let mut sql = String::new();
        for rel in nested {
            sql.push_str(&format!(" , {}", rel.to_subquery_sql(root_parent, "$parent")));
        }
        sql
    }
}
