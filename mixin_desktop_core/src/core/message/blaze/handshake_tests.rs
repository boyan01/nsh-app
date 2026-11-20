use super::*;
use crate::{db::app::AppDatabase, network::NetworkService};
use base64ct::{Base64UrlUnpadded, Encoding};
use tokio::{net::TcpListener, sync::oneshot};
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .map(|value| value.to_str().unwrap().to_owned())
}

#[tokio::test]
async fn blaze_handshake_sends_the_token_jti_as_its_request_id_and_the_device_id() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let directory = tempfile::tempdir().unwrap();
        let app = AppDatabase::connect_at(directory.path().join("app.db"))
            .await
            .unwrap();
        let database = Arc::new(
            MixinDatabase::connect_at(directory.path().join("mixin.db"))
                .await
                .unwrap(),
        );
        let network = Arc::new(
            NetworkService::new(app.setting_dao.clone(), "fixture-device".into())
                .await
                .unwrap(),
        );
        let credential = Credential::KeyStore(sdk::KeyStore {
            app_id: "fixture-user".into(),
            session_id: "fixture-session".into(),
            server_public_key: String::new(),
            session_private_key: "11".repeat(32),
            scp: "FULL".into(),
        });
        let blaze = Arc::new(
            Blaze::new(
                database.clone(),
                Arc::new(network.sdk_client(Credential::None)),
                credential,
                "fixture-user".into(),
                None,
            )
            .with_network(network),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = format!("ws://{}", listener.local_addr().unwrap());
        let (headers, received) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut headers = Some(headers);
            // Tungstenite's callback fixes its unboxed HTTP error response type.
            #[allow(clippy::result_large_err)]
            let callback = |request: &Request, mut response: Response| {
                let observed = ["Authorization", "X-Request-Id", "Mixin-Device-Id"]
                    .map(|name| header(request, name));
                headers.take().unwrap().send(observed).unwrap();
                response
                    .headers_mut()
                    .insert("Sec-WebSocket-Protocol", "Mixin-Blaze-1".parse().unwrap());
                Ok(response)
            };
            tokio_tungstenite::accept_hdr_async(stream, callback)
                .await
                .unwrap()
        });
        let client = blaze.clone();
        let run = tokio::spawn(async move { client.connect_hosts(&[&target]).await });
        let [authorization, request_id, device_id] = received.await.unwrap();
        assert_eq!(device_id.as_deref(), Some("fixture-device"));
        let token = authorization.unwrap();
        let claims = token
            .strip_prefix("Bearer ")
            .unwrap()
            .split('.')
            .nth(1)
            .unwrap();
        let claims: serde_json::Value =
            serde_json::from_slice(&Base64UrlUnpadded::decode_vec(claims).unwrap()).unwrap();
        let request_id = request_id.unwrap();
        assert!(uuid::Uuid::parse_str(&request_id).is_ok());
        assert_eq!(claims["jti"], request_id.as_str());
        run.abort();
        assert!(run.await.unwrap_err().is_cancelled());
        drop(server.await.unwrap());
        database.close().await;
    })
    .await
    .unwrap();
}
