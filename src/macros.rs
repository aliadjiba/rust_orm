#[macro_export]
macro_rules! run_migrations {
    ( $( $model:ty ),* $(,)? ) => {{
        use orm::model::Model;

        let mut sql = String::new();

        $(
            sql.push_str(<$model>::migration());
            sql.push('\n');
        )*

        sql
    }};
}