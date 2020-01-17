use hyper::Body;
use hyper_usse::EventBuilder;
use std::io::Write;
use tokio::sync::Mutex;
use std::collections::HashMap;
use futures::join;
use serde_json::Value;
use uuid::Uuid;

pub mod sse;
pub mod events;
mod content;

use events::{Subscribers, Subscription, AggregateSubscription, AbsolutePath, Path};
use content::Content;

/// A `Page` pairs some page `Content` (either dynamic or static) with a set of
/// `Subscribers` to the events on the page.
#[derive(Debug)]
pub struct Page {
    content: Mutex<Content>,
    subscribers: Mutex<Subscribers>,
}

/// The size of the dynamic page template in bytes
const TEMPLATE_SIZE: usize = include_str!("page/dynamic.html").len();

impl Page {
    /// Make a new empty (dynamic) page
    pub async fn new() -> Page {
        Page {
            content: Mutex::new(Content::new().await),
            subscribers: Mutex::new(Subscribers::new()),
        }
    }

    /// Render a whole page as HTML (for first page load).
    pub async fn render(&self) -> Vec<u8> {
        match &*self.content.lock().await {
            Content::Dynamic{title, body, ..} => {
                let subscribers = self.subscribers.lock().await;
                let aggregate_subscription = subscribers.total_subscription();
                let subscription =
                    serde_json::to_string(&aggregate_subscription).unwrap();
                let mut bytes = Vec::with_capacity(TEMPLATE_SIZE);
                write!(&mut bytes,
                       include_str!("page/dynamic.html"),
                       subscription = subscription,
                       debug = cfg!(debug_assertions),
                       title = title,
                       body = body)
                    .expect("Internal error: write!() failed on a Vec<u8>");
                bytes
            },
            Content::Static{raw_contents, ..} => {
                raw_contents.clone()
            },
        }
    }

    /// Subscribe another page event listener to this page, given a subscription
    /// specification for what events to listen to.
    pub async fn event_stream(
        &self, uuid: Option<Uuid>, subscription: Subscription,
    ) -> Option<(Uuid, Body)> {
        let mut subscribers = self.subscribers.lock().await;
        let (total_subscription, new_details) =
            subscribers.add_subscriber(uuid, subscription).await;
        let content = &mut *self.content.lock().await;
        match content {
            Content::Static{..} => { },
            Content::Dynamic{ref mut updates, ..} => {
                set_subscriptions(updates, total_subscription).await;
            }
        }
        new_details
    }

    /// Send an event to all subscribers. This should only be called with events
    /// that have come from the corresponding page itself, or confusion will
    /// result!
    pub async fn send_event(&self,
                            event_type: &str,
                            event_path: &AbsolutePath,
                            event_data: &HashMap<Path, Value>) {
        if let Some(total_subscription) =
            self.subscribers.lock().await
            .send_event(event_type, event_path, event_data).await {
                let content = &mut *self.content.lock().await;
                match content {
                    Content::Static{..} => { },
                    Content::Dynamic{ref mut updates, ..} => {
                        set_subscriptions(updates, total_subscription).await;
                    }
                }
            }
    }

    /// Send an empty "heartbeat" message to all clients of a page, if it is
    /// dynamic. This has no effect if it is (currently) static, and returns
    /// `None` if so, otherwise returns the current number of clients getting
    /// live updates to the page.
    pub async fn send_heartbeat(&self) -> Option<usize> {
        let mut subscribers = self.subscribers.lock().await;
        let mut content = self.content.lock().await;
        let subscriber_heartbeat = async {
            subscribers.send_heartbeat().await
        };
        let content_heartbeat = async {
            content.send_heartbeat().await
        };
        let (new_subscription, update_client_count) =
            join!(subscriber_heartbeat, content_heartbeat);
        if let Some(total_subscription) = new_subscription {
            match *content {
                Content::Dynamic{ref mut updates, ..} => {
                    set_subscriptions(updates, total_subscription).await;
                },
                Content::Static{..} => { },
            }
        }
        update_client_count
    }

    /// Test if this page is empty, where "empty" means that it is dynamic, with
    /// an empty title, empty body, and no subscribers waiting on its page
    /// events: that is, it's identical to `Page::new()`.
    pub async fn is_empty(&self) -> bool {
        let (content_empty, subscribers_empty) =
            join!(async { self.content.lock().await.is_empty().await },
                  async { self.subscribers.lock().await.is_empty() });
        content_empty && subscribers_empty
    }

    /// Add a client to the dynamic content of a page, if it is dynamic. If it
    /// is static, this has no effect and returns None. Otherwise, returns the
    /// Body stream to give to the new client.
    pub async fn update_stream(&self) -> Option<Body> {
        self.content.lock().await.update_stream().await
    }

    /// Set the contents of the page to be a static raw set of bytes with no
    /// self-refreshing functionality. All clients will be told to refresh their
    /// page to load the new static content (which will not be able to update
    /// itself until a client refreshes their page again).
    pub async fn set_static(&self,
                            content_type: Option<String>,
                            raw_contents: impl Into<Vec<u8>>) {
        self.content.lock().await.set_static(content_type, raw_contents).await
    }

    /// Get the content type of a page, or return `None` if none has been set
    /// (as in the case of a dynamic page, where the content type is not
    /// client-configurable).
    pub async fn content_type(&self) -> Option<String> {
        self.content.lock().await.content_type()
    }

    /// Tell all clients to change the title, if necessary. This converts the
    /// page into a dynamic page, overwriting any static content that previously
    /// existed, if any.
    pub async fn set_title(&self, new_title: impl Into<String>) {
        self.content.lock().await.set_title(new_title).await
    }

    /// Tell all clients to change the body, if necessary. This converts the
    /// page into a dynamic page, overwriting any static content that previously
    /// existed, if any.
    pub async fn set_body(&self, new_body: impl Into<String>) {
        self.content.lock().await.set_body(new_body).await
    }

    /// Clear the page entirely, removing all subscribers and resetting the page
    /// title and body to empty.
    pub async fn clear(&self) {
        let subscribers = &mut *self.subscribers.lock().await;
        *subscribers = Subscribers::new();
        let content = &mut *self.content.lock().await;
        match content {
            Content::Static{..} => { },
            Content::Dynamic{ref mut updates, ..} => {
                set_subscriptions(updates, AggregateSubscription::empty()).await;
            }
        }
        content.set_title("").await;
        content.set_body("").await;
    }
}

/// Send a new total set of subscriptions to the page, so it can update its
/// event hooks. This function should *only* ever be called *directly* after
/// obtaining such a new set of subscriptions from adding a subscriber,
/// sending an event, or sending a heartbeat! It will cause unexpected loss
/// of messages if you arbitrarily set the subscriptions of a page outside
/// of these contexts.
async fn set_subscriptions<'a>(server: &'a mut sse::BufferedServer,
                               subscription: AggregateSubscription<'a>) {
    let data = serde_json::to_string(&subscription)
        .expect("Serializing subscriptions to JSON shouldn't fail");
    let event = EventBuilder::new(&data).event_type("subscribe");
    // We're not using the future returned here because we don't care what the
    // number of client connections is, so we don't need to wait to find out
    let _unused_response = server.send_to_clients(event.build()).await;
}
