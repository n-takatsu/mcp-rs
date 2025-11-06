#[cfg(test)]
mod ultra_simple_tests {
    use crate::handlers::database::types::DatabaseType;

    #[test]
    fn test_basic_database_types() {
        // 基本的なEnum表示テスト
        let pg = DatabaseType::PostgreSQL;
        let mysql = DatabaseType::MySQL;

        assert_eq!(format!("{}", pg), "postgresql");
        assert_eq!(format!("{}", mysql), "mysql");
    }
}
