// 安全性を考慮したパフォーマンステストの例
// 実際のパフォーマンステストを再実装する際の参考実装

use mcp_rs::handlers::database::safety::{LoopGuard, SafetyError, SafetyManager};
use std::time::{Duration, Instant};
use tokio::time::timeout;

/// 安全なパフォーマンステストの実装例
pub struct SafePerformanceTest {
    safety_manager: SafetyManager,
}

impl SafePerformanceTest {
    pub fn new() -> Self {
        Self {
            safety_manager: SafetyManager::new(),
        }
    }

    /// 安全な基本クエリテスト
    pub async fn safe_basic_query_test(&self) -> Result<TestResult, SafetyError> {
        let max_iterations = 10; // 本来の100から削減
        let test_timeout = Duration::from_secs(30); // 全体タイムアウト

        let test_result = timeout(test_timeout, async {
            let mut results = Vec::new();
            let loop_guard = LoopGuard::new("basic_query_test", max_iterations);

            for i in 0..max_iterations {
                if !loop_guard.check_iteration() {
                    return Err(SafetyError::OperationFailed(
                        "Test loop limit exceeded".to_string(),
                    ));
                }

                let start = Instant::now();

                // 安全なクエリ実行（実際の実装では handler.call_tool を呼び出し）
                let _query_result = self
                    .safety_manager
                    .safe_execute(
                        || async {
                            // モッククエリ実行（実際のテストではDB操作）
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            Ok::<String, Box<dyn std::error::Error + Send + Sync>>(format!(
                                "Query {} result",
                                i
                            ))
                        },
                        &format!("query_{}", i),
                    )
                    .await?;

                let duration = start.elapsed();
                results.push(QueryTestResult {
                    iteration: i,
                    duration,
                    success: true, // モッククエリなので常に成功
                    result: format!("Query {} result", i),
                });

                // 間隔を空ける（DBサーバーへの負荷軽減）
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            Ok(TestResult {
                test_name: "safe_basic_query_test".to_string(),
                total_iterations: max_iterations,
                successful_iterations: results.iter().filter(|r| r.success).count() as u64,
                average_duration: Duration::from_nanos(
                    results
                        .iter()
                        .map(|r| r.duration.as_nanos() as u64)
                        .sum::<u64>()
                        / max_iterations,
                ),
                results,
            })
        })
        .await;

        match test_result {
            Ok(result) => result,
            Err(_) => Err(SafetyError::Timeout),
        }
    }

    /// 安全な並行テスト
    pub async fn safe_concurrent_test(&self) -> Result<TestResult, SafetyError> {
        let concurrent_tasks = 3; // 本来の10から削減
        let queries_per_task = 2; // 本来の5から削減
        let test_timeout = Duration::from_secs(45);

        let test_result = timeout(test_timeout, async {
            let mut task_handles = Vec::new();

            for task_id in 0..concurrent_tasks {
                let safety_manager = self.safety_manager.clone(); // cloneして所有権を渡す
                let handle = tokio::spawn(async move {
                    let mut task_results = Vec::new();
                    let loop_guard =
                        LoopGuard::new(&format!("concurrent_task_{}", task_id), queries_per_task);

                    for query_id in 0..queries_per_task {
                        if !loop_guard.check_iteration() {
                            break;
                        }

                        let start = Instant::now();
                        let _query_result = safety_manager
                            .safe_execute(
                                || async {
                                    tokio::time::sleep(Duration::from_millis(50)).await;
                                    Ok::<String, Box<dyn std::error::Error + Send + Sync>>(format!(
                                        "Task {} Query {} result",
                                        task_id, query_id
                                    ))
                                },
                                &format!("task_{}_query_{}", task_id, query_id),
                            )
                            .await;

                        let duration = start.elapsed();
                        task_results.push(QueryTestResult {
                            iteration: query_id,
                            duration,
                            success: true, // モッククエリなので常に成功
                            result: format!("Task {} Query {} result", task_id, query_id),
                        });
                    }

                    task_results
                });

                task_handles.push(handle);
            }

            // 全タスクの完了を待機
            let mut all_results = Vec::new();
            for handle in task_handles {
                match handle.await {
                    Ok(task_results) => all_results.extend(task_results),
                    Err(e) => {
                        return Err(SafetyError::OperationFailed(format!(
                            "Task execution failed: {}",
                            e
                        )))
                    }
                }
            }

            let successful_count = all_results.iter().filter(|r| r.success).count() as u64;
            let total_count = all_results.len() as u64;
            let avg_duration = if !all_results.is_empty() {
                Duration::from_nanos(
                    all_results
                        .iter()
                        .map(|r| r.duration.as_nanos() as u64)
                        .sum::<u64>()
                        / total_count,
                )
            } else {
                Duration::default()
            };

            Ok(TestResult {
                test_name: "safe_concurrent_test".to_string(),
                total_iterations: total_count,
                successful_iterations: successful_count,
                average_duration: avg_duration,
                results: all_results,
            })
        })
        .await;

        match test_result {
            Ok(result) => result,
            Err(_) => Err(SafetyError::Timeout),
        }
    }

    /// エラーハンドリングテスト（安全版）
    pub async fn safe_error_handling_test(&self) -> Result<TestResult, SafetyError> {
        let _error_scenarios = 3; // 本来の4から削減
        let iterations_per_scenario = 2; // 本来の5から削減
        let test_timeout = Duration::from_secs(20);

        let test_result = timeout(test_timeout, async {
            let mut all_results = Vec::new();
            let scenarios = vec![
                "INVALID SQL QUERY",
                "SELECT * FROM non_existent_table",
                "INSERT INTO",
            ];

            for (scenario_idx, scenario) in scenarios.iter().enumerate() {
                let loop_guard = LoopGuard::new(
                    &format!("error_scenario_{}", scenario_idx),
                    iterations_per_scenario,
                );

                for iteration in 0..iterations_per_scenario {
                    if !loop_guard.check_iteration() {
                        break;
                    }

                    let start = Instant::now();

                    // 意図的にエラーを発生させるテスト
                    let _error_result = self
                        .safety_manager
                        .safe_execute(
                            || async {
                                // モックエラー（実際のテストではDB操作）
                                Err::<String, Box<dyn std::error::Error + Send + Sync>>(Box::new(
                                    std::io::Error::new(
                                        std::io::ErrorKind::InvalidInput,
                                        format!("Mock error for: {}", scenario),
                                    ),
                                ))
                            },
                            &format!("error_test_{}_{}", scenario_idx, iteration),
                        )
                        .await;

                    let duration = start.elapsed();
                    all_results.push(QueryTestResult {
                        iteration: (iteration as u64
                            + scenario_idx as u64 * iterations_per_scenario as u64),
                        duration,
                        success: false, // エラーテストなので成功は期待しない
                        result: format!("Expected error for: {}", scenario),
                    });

                    // エラーハンドリングテストでも間隔を空ける
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }

            let total_count = all_results.len() as u64;
            let avg_duration = if !all_results.is_empty() {
                Duration::from_nanos(
                    all_results
                        .iter()
                        .map(|r| r.duration.as_nanos() as u64)
                        .sum::<u64>()
                        / total_count,
                )
            } else {
                Duration::default()
            };

            Ok(TestResult {
                test_name: "safe_error_handling_test".to_string(),
                total_iterations: total_count,
                successful_iterations: 0, // エラーテストなので0が期待値
                average_duration: avg_duration,
                results: all_results,
            })
        })
        .await;

        match test_result {
            Ok(result) => result,
            Err(_) => Err(SafetyError::Timeout),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueryTestResult {
    pub iteration: u64,
    pub duration: Duration,
    pub success: bool,
    pub result: String,
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub test_name: String,
    pub total_iterations: u64,
    pub successful_iterations: u64,
    pub average_duration: Duration,
    pub results: Vec<QueryTestResult>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_safe_performance_basic() {
        let test_runner = SafePerformanceTest::new();

        let result = test_runner.safe_basic_query_test().await;
        assert!(result.is_ok(), "Safe basic query test should succeed");

        let test_result = result.unwrap();
        assert_eq!(test_result.test_name, "safe_basic_query_test");
        assert!(test_result.total_iterations > 0);
        println!("✅ Safe basic test completed: {:?}", test_result);
    }

    #[tokio::test]
    async fn test_safe_performance_concurrent() {
        let test_runner = SafePerformanceTest::new();

        let result = test_runner.safe_concurrent_test().await;
        assert!(result.is_ok(), "Safe concurrent test should succeed");

        let test_result = result.unwrap();
        assert_eq!(test_result.test_name, "safe_concurrent_test");
        println!("✅ Safe concurrent test completed: {:?}", test_result);
    }

    #[tokio::test]
    async fn test_safe_performance_error_handling() {
        let test_runner = SafePerformanceTest::new();

        let result = test_runner.safe_error_handling_test().await;
        assert!(result.is_ok(), "Safe error handling test should succeed");

        let test_result = result.unwrap();
        assert_eq!(test_result.test_name, "safe_error_handling_test");
        // エラーテストなので成功数は0が期待値
        assert_eq!(test_result.successful_iterations, 0);
        println!("✅ Safe error handling test completed: {:?}", test_result);
    }
}
