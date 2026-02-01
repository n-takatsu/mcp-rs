//! PostgreSQL JSONB Performance Benchmarks
//!
//! Benchmarks for JSONB operations and comparisons

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::json;

#[cfg(feature = "postgresql-backend")]
mod jsonb_benchmarks {
    use super::*;
    use mcp_rs::handlers::database::engines::postgres::{
        JsonbHandler, JsonbQueryBuilder, PostgresConfig, PostgresEngine,
    };

    async fn setup_engine() -> PostgresEngine {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_bench")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        PostgresEngine::new(config)
            .await
            .expect("Failed to create engine")
    }

    async fn setup_test_data(handler: &JsonbHandler) {
        // Create test table
        let pool = handler.pool.clone();
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bench_jsonb (
                id SERIAL PRIMARY KEY,
                data JSONB,
                metadata JSONB
            );
            TRUNCATE bench_jsonb;
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create table");

        // Insert test data
        for i in 0..1000 {
            let data = json!({
                "id": i,
                "name": format!("User {}", i),
                "age": 20 + (i % 50),
                "city": if i % 3 == 0 { "Tokyo" } else if i % 3 == 1 { "Osaka" } else { "Kyoto" },
                "interests": ["coding", "music", "sports"],
                "active": i % 2 == 0
            });

            handler
                .insert_jsonb("bench_jsonb", "data", &data)
                .await
                .expect("Failed to insert data");
        }

        // Create GIN index
        handler
            .create_gin_index("bench_jsonb", "data", Some("bench_data_gin"))
            .await
            .expect("Failed to create index");
    }

    pub fn bench_jsonb_insert(c: &mut Criterion) {
        let rt = tokio::runtime::Runtime::new().unwrap();

        c.bench_function("jsonb_insert", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let engine = setup_engine().await;
                    let handler = engine.jsonb_handler();

                    let data = json!({
                        "name": "Benchmark User",
                        "age": 30,
                        "tags": ["test", "benchmark"]
                    });

                    handler
                        .insert_jsonb("bench_jsonb", "data", &data)
                        .await
                        .expect("Insert failed");
                })
            });
        });
    }

    pub fn bench_jsonb_query_with_gin(c: &mut Criterion) {
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let engine = setup_engine().await;
            let handler = engine.jsonb_handler();
            setup_test_data(&handler).await;
        });

        c.bench_function("jsonb_query_with_gin_index", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let engine = setup_engine().await;
                    let handler = engine.jsonb_handler();

                    handler
                        .query_jsonb_path(
                            "bench_jsonb",
                            "data",
                            "city",
                            Some("data->>'city' = 'Tokyo'"),
                        )
                        .await
                        .expect("Query failed");
                })
            });
        });
    }

    pub fn bench_jsonb_contains_operator(c: &mut Criterion) {
        let rt = tokio::runtime::Runtime::new().unwrap();

        c.bench_function("jsonb_contains_operator", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let engine = setup_engine().await;
                    let pool = engine.pool();

                    let result = sqlx::query("SELECT COUNT(*) FROM bench_jsonb WHERE data @> $1")
                        .bind(json!({"active": true}))
                        .fetch_one(pool)
                        .await
                        .expect("Query failed");

                    black_box(result);
                })
            });
        });
    }

    pub fn bench_jsonb_update_field(c: &mut Criterion) {
        let rt = tokio::runtime::Runtime::new().unwrap();

        c.bench_function("jsonb_update_field", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let engine = setup_engine().await;
                    let handler = engine.jsonb_handler();

                    handler
                        .update_jsonb_field(
                            "bench_jsonb",
                            "data",
                            "{age}",
                            &json!(31),
                            Some("id = 1"),
                        )
                        .await
                        .expect("Update failed");
                })
            });
        });
    }

    pub fn bench_jsonb_aggregation(c: &mut Criterion) {
        let rt = tokio::runtime::Runtime::new().unwrap();

        c.bench_function("jsonb_aggregation", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let engine = setup_engine().await;
                    let handler = engine.jsonb_handler();

                    handler
                        .aggregate_jsonb("bench_jsonb", "data", Some("data->>'city' = 'Tokyo'"))
                        .await
                        .expect("Aggregation failed");
                })
            });
        });
    }

    pub fn bench_query_builder(c: &mut Criterion) {
        c.bench_function("query_builder_construction", |b| {
            b.iter(|| {
                let builder = JsonbQueryBuilder::new("events", "metadata")
                    .has_key("timestamp")
                    .contains(json!({"status": "active"}))
                    .has_any_key(&["user_id", "session_id"]);

                black_box(builder.build_select());
            });
        });
    }

    pub fn bench_jsonb_path_extraction(c: &mut Criterion) {
        let rt = tokio::runtime::Runtime::new().unwrap();

        let mut group = c.benchmark_group("jsonb_path_extraction");

        for depth in [1, 2, 3, 4].iter() {
            group.bench_with_input(BenchmarkId::from_parameter(depth), depth, |b, &depth| {
                b.iter(|| {
                    rt.block_on(async {
                        let engine = setup_engine().await;
                        let pool = engine.pool();

                        let path = (0..depth).map(|_| "->''").collect::<String>();
                        let sql = format!("SELECT data{} FROM bench_jsonb LIMIT 100", path);

                        let result = sqlx::query(&sql)
                            .fetch_all(pool)
                            .await
                            .expect("Query failed");

                        black_box(result);
                    })
                });
            });
        }
        group.finish();
    }
}

#[cfg(feature = "postgresql-backend")]
criterion_group!(
    benches,
    jsonb_benchmarks::bench_jsonb_insert,
    jsonb_benchmarks::bench_jsonb_query_with_gin,
    jsonb_benchmarks::bench_jsonb_contains_operator,
    jsonb_benchmarks::bench_jsonb_update_field,
    jsonb_benchmarks::bench_jsonb_aggregation,
    jsonb_benchmarks::bench_query_builder,
    jsonb_benchmarks::bench_jsonb_path_extraction
);

#[cfg(not(feature = "postgresql-backend"))]
criterion_group!(benches,);

criterion_main!(benches);
