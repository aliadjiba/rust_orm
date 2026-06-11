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
    pub fn parse_path(path: &str) -> Vec<NestedRelation> {
        path.split(',')
            .filter(|s| !s.is_empty())
            .map(|segment| {
                let parts: Vec<&str> = segment.split('.').filter(|s| !s.is_empty()).collect();
                let mut root = NestedRelation {
                    name: parts[0].to_string(),
                    children: Vec::new(),
                };
                let mut current = &mut root;
                for part in &parts[1..] {
                    current.children.push(NestedRelation {
                        name: part.to_string(),
                        children: Vec::new(),
                    });
                    current = current.children.last_mut().unwrap();
                }
                root
            })
            .collect()
    }

    pub fn to_subquery_sql(&self, parent_table: &str, parent_ref: &str) -> String {
        let singular = pluralizer::singularize(&self.name);
        let lookup_name = self.name.to_lowercase();

        if let Some(rel) = Relation::get(parent_table, &lookup_name) {
            match rel.relation_type {
                // RelationType::BelongsToMany => {
                //     let child_table = rel.child_table.as_deref().unwrap_or(&singular);
                //     let pivot_table = rel.pivot.as_deref().unwrap_or("");
                //     let (pivot_fk, pivot_child_key) = if rel.is_left.unwrap_or(true) {
                //         (
                //             rel.pivot_left_key.as_deref().unwrap_or(""),
                //             rel.pivot_right_key.as_deref().unwrap_or(""),
                //         )
                //     } else {
                //         (
                //             rel.pivot_right_key.as_deref().unwrap_or(""),
                //             rel.pivot_left_key.as_deref().unwrap_or(""),
                //         )
                //     };

                //     if self.children.is_empty() {
                //         return format!(
                //             "(SELECT VALUE {pivot_child_key}.* FROM {pivot_table} WHERE {pivot_fk} = $parent.id) AS {}",
                //             self.name
                //         );
                //     }

                //     let mut inner = String::from("SELECT *");
                //     for child in &self.children {
                //         inner.push_str(&format!(" , {} ", child.to_subquery_sql(child_table, "$parent")));
                //     }
                //     return format!(
                //         "(SELECT VALUE ({inner} FROM {child_table} WHERE id = $parent.{pivot_child_key})[0] FROM {pivot_table} WHERE {pivot_fk} = $parent.id) AS {}",
                //         self.name
                //     );
                // }
                RelationType::BelongsToMany => {
                    let pivot_table = rel.pivot.as_deref().unwrap_or("");
                    let (pivot_fk, pivot_child_key) = if rel.is_left.unwrap_or(true) {
                        (
                            rel.pivot_left_key.as_deref().unwrap_or(""),
                            rel.pivot_right_key.as_deref().unwrap_or(""),
                        )
                    } else {
                        (
                            rel.pivot_right_key.as_deref().unwrap_or(""),
                            rel.pivot_left_key.as_deref().unwrap_or(""),
                        )
                    };

                    if self.children.is_empty() {
                        // e.g. ?with=ingredients
                        return format!(
                            "(SELECT * OMIT {pivot_fk} FROM {pivot_table} WHERE {pivot_fk} = $parent.id FETCH {pivot_child_key}) AS {}",
                            self.name
                        );
                    }

                    // e.g. ?with=ingredients.family  →  FETCH ingredient, ingredient.family
                    let mut fetches = vec![pivot_child_key.to_string()];
                    for child in &self.children {
                        fetches.push(format!("{pivot_child_key}.{}", child.name));
                    }
                    let fetch_clause = format!("FETCH {}", fetches.join(", "));  // ← comma-separated, single FETCH

                    return format!(
                        "(SELECT * OMIT {pivot_fk} FROM {pivot_table} WHERE {pivot_fk} = $parent.id {fetch_clause}) AS {}",
                        self.name
                    );
                }
                RelationType::HasMany => {
                    let child_table = rel.child_table.as_deref().unwrap_or(&singular);
                    let mut inner = String::from("SELECT *");
                    for child in &self.children {
                        inner.push_str(&format!(" , {} ", child.to_subquery_sql(child_table, "$parent")));
                    }
                    return format!(
                        "({inner} FROM {child_table} WHERE {parent_table} = {parent_ref}.id) AS {}",
                        self.name
                    );
                }
                RelationType::BelongsTo => {
                    let mut inner = String::from("SELECT *");
                    for child in &self.children {
                        inner.push_str(&format!(" , {} ", child.to_subquery_sql(&singular, "$parent")));
                    }
                    return format!(
                        "({inner} FROM {singular} WHERE id = {parent_ref}.{})[0] AS {}",
                        self.name, self.name
                    );
                }
            }
        }

        // Fallback (no registered relation)
        let mut inner = String::from("SELECT *");
        for child in &self.children {
            inner.push_str(&format!(" , {} ", child.to_subquery_sql(&singular, "$parent")));
        }
        let condition = if pluralizer::is_plural(&self.name) {
            format!("{parent_table} = {parent_ref}.id")
        } else {
            format!("id = {parent_ref}.{}", self.name)
        };
        format!("({inner} FROM {singular} WHERE {condition}) AS {}", self.name)
    }
    pub fn generate_nested_sql(nested: &[NestedRelation], root_parent: &str) -> String {
        let mut sql = String::new();
        for rel in nested {
            sql.push_str(&format!(" , {} ", rel.to_subquery_sql(root_parent, "$parent")));
        }
        sql
    }
}
