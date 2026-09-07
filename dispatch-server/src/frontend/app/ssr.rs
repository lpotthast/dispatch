//! Keep Leptonic's thread-local render owners on one worker through stream disposal.

use axum::{
    body::{Body, Bytes},
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use futures_util::{StreamExt, stream};
use tokio::sync::{mpsc, oneshot};
use tokio_util::task::LocalPoolHandle;

pub(crate) async fn render_on_worker(
    State(pool): State<LocalPoolHandle>,
    request: Request,
    next: Next,
) -> Response {
    let (head_tx, head_rx) = oneshot::channel();
    let (body_tx, body_rx) = mpsc::channel::<Result<Bytes, axum::Error>>(1);
    // The response stream owns reactive cleanup. Moving only the handler onto a
    // worker is insufficient: both consumption and cancellation must happen there.
    pool.spawn_pinned(move || async move {
        let (parts, body) = next.run(request).await.into_parts();
        if head_tx.send(parts).is_err() {
            return;
        }
        let mut stream = body.into_data_stream();
        while let Some(chunk) = stream.next().await {
            if body_tx.send(chunk).await.is_err() {
                break;
            }
        }
    });
    match head_rx.await {
        Ok(parts) => Response::from_parts(
            parts,
            Body::from_stream(stream::unfold(body_rx, |mut receiver| async move {
                receiver.recv().await.map(|chunk| (chunk, receiver))
            })),
        ),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use axum::{Router, middleware, routing::get};
    use leptos::prelude::*;
    use std::rc::Rc;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_render_streams_preserve_thread_local_owners() {
        let app = Router::new()
            .route(
                "/",
                get(|| async {
                    let owner = Owner::new();
                    let value = owner.with(|| StoredValue::new_local(Rc::new("rendered")));
                    Body::from_stream(stream::once(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                        let body =
                            value.with_value(|value| Bytes::copy_from_slice(value.as_bytes()));
                        owner.cleanup();
                        Ok::<_, std::io::Error>(body)
                    }))
                }),
            )
            .layer(middleware::from_fn_with_state(
                LocalPoolHandle::new(2),
                render_on_worker,
            ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(axum::serve(listener, app).into_future());
        let client = reqwest::Client::new();
        let requests = (0..16).map(|_| {
            let client = client.clone();
            let url = url.clone();
            tokio::spawn(async move {
                let response = client.get(url).send().await.unwrap();
                assert_that!(response.status()).is_equal_to(StatusCode::OK);
                let body = response.bytes().await.unwrap();
                assert_that!(body.as_ref()).is_equal_to(b"rendered".as_slice());
            })
        });
        for result in futures_util::future::join_all(requests).await {
            result.unwrap();
        }
        server.abort();
    }
}
