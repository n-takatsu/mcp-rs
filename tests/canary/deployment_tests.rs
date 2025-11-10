#[cfg(test)]
mod canary_tests {
    use super::*;
    use crate::canary_deployment::{
        CanaryDeploymentManager, CanaryEventType, DeploymentState, RequestContext, SplitCriteria,
        UserGroup,
    };
    use crate::policy_config::PolicyConfig;
    use std::collections::HashMap;
    use tokio::time::Duration;

    fn create_test_policy(id: &str, name: &str) -> PolicyConfig {
        PolicyConfig {
            id: id.to_string(),
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test policy".to_string()),
            ..Default::default()
        }
    }

    fn create_test_context(user_id: &str) -> RequestContext {
        RequestContext {
            request_id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            ip_address: "192.168.1.100".to_string(),
            user_agent: Some("test-agent".to_string()),
            custom_headers: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_canary_deployment_manager_creation() {
        let policy = create_test_policy("test-1", "Test Policy");
        let manager = CanaryDeploymentManager::new(policy.clone());

        // 初期状態の確認
        assert_eq!(manager.get_deployment_state(), DeploymentState::Idle);
        assert_eq!(manager.get_stable_policy().id, policy.id);
        assert!(manager.get_canary_policy().is_none());
    }

    #[tokio::test]
    async fn test_start_canary_deployment() {
        let stable_policy = create_test_policy("stable-1", "Stable Policy");
        let canary_policy = create_test_policy("canary-1", "Canary Policy");
        let manager = CanaryDeploymentManager::new(stable_policy);

        // カナリアデプロイメント開始
        let result = manager
            .start_canary_deployment(canary_policy.clone(), 10.0)
            .await;
        assert!(result.is_ok());

        // 状態確認
        match manager.get_deployment_state() {
            DeploymentState::CanaryActive { percentage, .. } => {
                assert_eq!(percentage, 10.0);
            }
            _ => panic!("Expected CanaryActive state"),
        }

        // カナリアポリシーが設定されているか確認
        let current_canary = manager.get_canary_policy();
        assert!(current_canary.is_some());
        assert_eq!(current_canary.unwrap().id, canary_policy.id);
    }

    #[tokio::test]
    async fn test_traffic_split_random() {
        let stable_policy = create_test_policy("stable-1", "Stable Policy");
        let canary_policy = create_test_policy("canary-1", "Canary Policy");
        let manager = CanaryDeploymentManager::new(stable_policy);

        // カナリアデプロイメント開始（50%分散）
        manager
            .start_canary_deployment(canary_policy, 50.0)
            .await
            .unwrap();

        // 多数のリクエストで分散をテスト
        let mut canary_count = 0;
        let total_requests = 1000;

        for i in 0..total_requests {
            let context = create_test_context(&format!("user_{}", i));
            if manager.should_use_canary(&context) {
                canary_count += 1;
            }
        }

        let canary_percentage = canary_count as f32 / total_requests as f32 * 100.0;

        // 50% ± 10%の範囲であることを確認（統計的な許容範囲）
        assert!(
            (40.0..=60.0).contains(&canary_percentage),
            "Canary percentage {} is outside expected range 40-60%",
            canary_percentage
        );
    }

    #[tokio::test]
    async fn test_user_id_hash_consistency() {
        let stable_policy = create_test_policy("stable-1", "Stable Policy");
        let canary_policy = create_test_policy("canary-1", "Canary Policy");
        let manager = CanaryDeploymentManager::new(stable_policy);

        // UserIdHashベースの分散に変更
        {
            let traffic_split = manager.get_traffic_split();
            let mut split = traffic_split.write().unwrap();
            split.criteria = SplitCriteria::UserIdHash;
            split.canary_percentage = 50.0;
        }

        manager
            .start_canary_deployment(canary_policy, 50.0)
            .await
            .unwrap();

        // 同じユーザーIDは一貫した結果を返すことを確認
        let context1 = create_test_context("consistent_user");
        let context2 = create_test_context("consistent_user");

        let result1 = manager.should_use_canary(&context1);
        let result2 = manager.should_use_canary(&context2);

        assert_eq!(result1, result2, "Same user should get consistent routing");
    }

    #[tokio::test]
    async fn test_traffic_split_update() {
        let stable_policy = create_test_policy("stable-1", "Stable Policy");
        let canary_policy = create_test_policy("canary-1", "Canary Policy");
        let manager = CanaryDeploymentManager::new(stable_policy);

        // カナリアデプロイメント開始
        manager
            .start_canary_deployment(canary_policy, 10.0)
            .await
            .unwrap();

        // トラフィック分散を更新
        let result = manager.update_traffic_split(25.0).await;
        assert!(result.is_ok());

        // 更新された分散率を確認
        let traffic_split = manager.get_traffic_split();
        let split = traffic_split.read().unwrap();
        assert_eq!(split.canary_percentage, 25.0);
    }

    #[tokio::test]
    async fn test_invalid_traffic_percentage() {
        let stable_policy = create_test_policy("stable-1", "Stable Policy");
        let manager = CanaryDeploymentManager::new(stable_policy);

        // 無効な割合でのトラフィック分散更新をテスト
        let result = manager.update_traffic_split(-5.0).await;
        assert!(result.is_err());

        let result = manager.update_traffic_split(105.0).await;
        assert!(result.is_err());

        // 有効な範囲でのトラフィック分散更新をテスト
        let result = manager.update_traffic_split(50.0).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_metrics_recording() {
        let stable_policy = create_test_policy("stable-1", "Stable Policy");
        let manager = CanaryDeploymentManager::new(stable_policy);

        // メトリクスを記録
        manager.record_request_metrics(false, true, 100); // 安定版、成功、100ms
        manager.record_request_metrics(false, false, 200); // 安定版、失敗、200ms
        manager.record_request_metrics(true, true, 80); // カナリア版、成功、80ms

        // メトリクス確認
        let metrics_collector = manager.get_metrics_collector();
        let collector = metrics_collector.read().unwrap();

        // 安定版メトリクス
        assert_eq!(collector.stable_metrics.total_requests, 2);
        assert_eq!(collector.stable_metrics.successful_requests, 1);
        assert_eq!(collector.stable_metrics.error_requests, 1);
        assert_eq!(collector.stable_metrics.success_rate(), 50.0);

        // カナリア版メトリクス
        assert_eq!(collector.canary_metrics.total_requests, 1);
        assert_eq!(collector.canary_metrics.successful_requests, 1);
        assert_eq!(collector.canary_metrics.error_requests, 0);
        assert_eq!(collector.canary_metrics.success_rate(), 100.0);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let stable_policy = create_test_policy("stable-1", "Stable Policy");
        let canary_policy = create_test_policy("canary-1", "Canary Policy");
        let manager = CanaryDeploymentManager::new(stable_policy);

        // イベント購読
        let mut receiver = manager.subscribe();

        // カナリアデプロイメント開始（非同期でイベントをチェック）
        let manager_clone = &manager;
        let deployment_task = async move {
            manager_clone
                .start_canary_deployment(canary_policy, 15.0)
                .await
        };

        let event_task =
            async move { tokio::time::timeout(Duration::from_secs(1), receiver.recv()).await };

        // 両方のタスクを実行
        let (deployment_result, event_result) = tokio::join!(deployment_task, event_task);

        assert!(deployment_result.is_ok());
        assert!(event_result.is_ok());

        let event = event_result.unwrap().unwrap();
        match event.event_type {
            CanaryEventType::CanaryStarted { percentage } => {
                assert_eq!(percentage, 15.0);
            }
            _ => panic!("Expected CanaryStarted event"),
        }
    }

    #[tokio::test]
    async fn test_concurrent_deployment_rejection() {
        let stable_policy = create_test_policy("stable-1", "Stable Policy");
        let canary_policy1 = create_test_policy("canary-1", "Canary Policy 1");
        let canary_policy2 = create_test_policy("canary-2", "Canary Policy 2");
        let manager = CanaryDeploymentManager::new(stable_policy);

        // 最初のカナリアデプロイメント開始
        let result1 = manager.start_canary_deployment(canary_policy1, 10.0).await;
        assert!(result1.is_ok());

        // 2番目のカナリアデプロイメント試行（失敗するはず）
        let result2 = manager.start_canary_deployment(canary_policy2, 20.0).await;
        assert!(result2.is_err());
    }

    #[tokio::test]
    async fn test_force_canary_user_groups() {
        let stable_policy = create_test_policy("stable-1", "Stable Policy");
        let canary_policy = create_test_policy("canary-1", "Canary Policy");
        let manager = CanaryDeploymentManager::new(stable_policy);

        // 強制カナリアグループを設定
        {
            let traffic_split = manager.get_traffic_split();
            let mut split = traffic_split.write().unwrap();
            split.user_groups.push(UserGroup {
                name: "beta_testers".to_string(),
                users: vec!["beta_user_1".to_string(), "beta_user_2".to_string()],
                force_canary: true,
            });
            split.canary_percentage = 0.0; // 通常は0%
        }

        manager
            .start_canary_deployment(canary_policy, 0.0)
            .await
            .unwrap();

        // 強制カナリアユーザーはカナリア版を取得
        let beta_context = create_test_context("beta_user_1");
        assert!(manager.should_use_canary(&beta_context));

        // 通常ユーザーは安定版を取得
        let normal_context = create_test_context("normal_user");
        assert!(!manager.should_use_canary(&normal_context));
    }
}
