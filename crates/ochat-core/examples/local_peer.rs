//! Local diagnostic peer for testing the desktop UI without a second device.
//! Usage: cargo run -p ochat-core --example local_peer -- <invitation-file>
use ochat_core::{ChatNode, Event};
use std::{path::PathBuf, time::Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("Pass an output path for the temporary invitation"))?,
    );
    let node = ChatNode::start("Local test peer", false).await?;
    let invite = node.create_room("Local test", 4).await?;
    tokio::fs::write(&path, invite).await?;
    println!(
        "Local test peer ready. Invitation written to {}. Exits after 10 minutes.",
        path.display()
    );
    let mut events = node.subscribe();
    let end = tokio::time::sleep(Duration::from_secs(600));
    tokio::pin!(end);
    let mut greeted = false;
    loop {
        tokio::select! {
            _ = &mut end => break,
            event = events.recv() => match event? {
                Event::State { snapshot } if !snapshot.peers.is_empty() && !greeted => {
                    greeted = true;
                    node.send_text("Local connection verified. This message comes from a second Rust process over encrypted QUIC.").await?;
                }
                Event::Message { peer, text, .. } if peer != node.id() => {
                    node.send_text(&format!("Message received: {text}")).await?;
                }
                _ => {}
            }
        }
    }
    node.close().await;
    tokio::fs::remove_file(path).await?;
    Ok(())
}
