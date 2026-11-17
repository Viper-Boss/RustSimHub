// SPDX-License-Identifier: AGPL-3.0-only
//! Serve the original HTTPS router only. Never start AirSIM/BLE/media listeners.
use super::*;

#[tokio::test]
#[ignore = "explicit private loopback Core/Go NAS fixture only"]
async fn original_edge_fixture() {
    assert_eq!(std::env::var("RSH_A3_NAS").unwrap(), "1");
    let root = PathBuf::from(std::env::var_os("RSH_A3_NAS_CORE").unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let authority = listener.local_addr().unwrap().to_string();
    let origin = format!("https://{authority}");
    let mut app = app(root.join("core.sock"));
    app.origins = Arc::from([(
        Box::<str>::from(origin.clone()),
        Box::<str>::from(authority),
    )]);
    create_tls(&root.join("tls"), &["127.0.0.1".into()]).unwrap();
    let acceptor = tls(&root.join("tls")).unwrap();
    let router = router(app);
    std::fs::write(
        root.join("edge.json"),
        serde_json::to_vec(&serde_json::json!({"origin":origin})).unwrap(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut tasks = JoinSet::new();
    loop {
        assert!(
            Instant::now() < deadline,
            "Edge fixture exceeded bounded deadline"
        );
        if root.join("done").exists() {
            break;
        }
        tokio::select! {
            incoming=listener.accept()=>{
                let (stream,_)=incoming.unwrap();let acceptor=acceptor.clone();let routes=router.clone();
                assert!(tasks.len()<64);
                tasks.spawn(async move {
                    if let Ok(Ok(stream))=timeout(Duration::from_secs(3),acceptor.accept(stream)).await {
                        let service=TowerToHyperService::new(routes);
                        let _=timeout(Duration::from_secs(15),hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream),service)).await;
                    }
                });
            }
            _=tasks.join_next(),if !tasks.is_empty()=>{},
            _=tokio::time::sleep(Duration::from_millis(20))=>{},
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
}
