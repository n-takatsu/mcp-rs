//! プラグイン隔離システムのベンチマーク - Issue #190

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use mcp_rs::plugin_isolation::{
    lifecycle_manager::LifecycleManager,
    IsolatedPluginManager, PluginManagerConfig,
    monitoring::MonitoringSystem,
    PluginMetadata, ResourceLimits, SecurityLevel,
};
use uuid::Uuid;

/// ベンチマーク1: プラグインマネージャーの作成
fn bench_plugin_manager_creation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("plugin_manager_creation", |b| {
        b.iter(|| {
            rt.block_on(async {
                let config = PluginManagerConfig::default();
                let _manager = IsolatedPluginManager::new(black_box(config)).await.unwrap();
            })
        })
    });
}

/// ベンチマーク2: プラグインメタデータの作成
fn bench_metadata_creation(c: &mut Criterion) {
    c.bench_function("metadata_creation", |b| {
        b.iter(|| {
            black_box(PluginMetadata {
                id: Uuid::new_v4(),
                name: "test-plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "Test plugin".to_string(),
                author: "Test Author".to_string(),
                required_permissions: vec![],
                resource_limits: ResourceLimits::default(),
                security_level: SecurityLevel::Standard,
                dependencies: vec![],
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
        })
    });
}

/// ベンチマーク3: ライフサイクルマネージャーの初期化
fn bench_lifecycle_manager_init(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("lifecycle_manager_init", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _manager = LifecycleManager::new().await.unwrap();
            })
        })
    });
}

/// ベンチマーク4: モニタリングシステムの初期化
fn bench_monitoring_system_init(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("monitoring_system_init", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _system = MonitoringSystem::new().await.unwrap();
            })
        })
    });
}

/// ベンチマーク5: リソース制限のクローン
fn bench_resource_limits_clone(c: &mut Criterion) {
    let limits = ResourceLimits::default();
    
    c.bench_function("resource_limits_clone", |b| {
        b.iter(|| {
            black_box(limits.clone())
        })
    });
}

/// ベンチマーク6: UUID生成
fn bench_uuid_generation(c: &mut Criterion) {
    c.bench_function("uuid_generation", |b| {
        b.iter(|| {
            black_box(Uuid::new_v4())
        })
    });
}

/// ベンチマーク7: 複数メタデータの作成(スケーラビリティテスト)
fn bench_multiple_metadata_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("metadata_scalability");
    
    for count in [10, 50, 100, 500].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter(|| {
                let plugins: Vec<_> = (0..count)
                    .map(|i| PluginMetadata {
                        id: Uuid::new_v4(),
                        name: format!("plugin-{}", i),
                        version: "1.0.0".to_string(),
                        description: format!("Plugin {}", i),
                        author: "Test".to_string(),
                        required_permissions: vec![],
                        resource_limits: ResourceLimits::default(),
                        security_level: SecurityLevel::Standard,
                        dependencies: vec![],
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    })
                    .collect();
                black_box(plugins)
            })
        });
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_plugin_manager_creation,
    bench_metadata_creation,
    bench_lifecycle_manager_init,
    bench_monitoring_system_init,
    bench_resource_limits_clone,
    bench_uuid_generation,
    bench_multiple_metadata_creation
);

criterion_main!(benches);
