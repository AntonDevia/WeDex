use wedex_cloud::{admin, client, server};
#[tokio::test(flavor = "multi_thread")]
async fn only_infrastructure_owner_may_update_neither_native_nor_browser_devices_can() {
    let dir = tempfile::tempdir().unwrap();
    let server = server::start(server::Config {
        data_dir: dir.path().into(),
        bind: "127.0.0.1:0".parse().unwrap(),
    })
    .unwrap();
    let phone = client::pair(&server.invitation(false).unwrap(), "Phone", true)
        .await
        .unwrap();
    let browser = client::pair(&server.invitation(false).unwrap(), "Browser", false)
        .await
        .unwrap();
    let http = reqwest::Client::builder()
        .tls_built_in_root_certs(false)
        .add_root_certificate(
            reqwest::Certificate::from_pem(server.certificate.as_bytes()).unwrap(),
        )
        .build()
        .unwrap();
    let url = format!("{}api/update/status", server.endpoint);
    assert_eq!(
        http.get(&url)
            .bearer_auth(&phone.credentials.token)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    for token in [&browser.credentials.token, "not-a-token"] {
        assert_eq!(
            http.get(&url)
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        assert_eq!(
            http.post(format!("{}api/update", server.endpoint))
                .bearer_auth(token)
                .json(&serde_json::json!({}))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
    }
    // Native account devices cannot administer a shared server.
    assert_eq!(
        http.post(format!("{}api/update", server.endpoint))
            .bearer_auth(&phone.credentials.token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let owner = admin::Owner {
        endpoint: server.endpoint.clone(),
        certificate: server.certificate.clone(),
        owner: server.owner_token().into(),
    };
    owner.revoke(&phone.credentials.device_id).await.unwrap();
    assert_eq!(
        http.get(&url)
            .bearer_auth(&phone.credentials.token)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        http.post(format!("{}api/update", server.endpoint))
            .bearer_auth(server.owner_token())
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
}
