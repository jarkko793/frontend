/// Public module `server` containing related server-side functionality.
pub mod server;

use anyhow::{Result, anyhow};
use ap_client_backend_v2::backend::ListOfDiscoveredEdgeNodes;
use ap_client_backend_v2::backend::UnreadMessagesFromServer;
use ap_client_backend_v2::backend::{Command, Service};
use crossbeam_channel::{Receiver, Sender, unbounded};
use messages::{node::NodeOptions, node_event::NodeEvent};
use std::thread;

/// `Client` is the main interface for interacting with the backend.
/// It manages channels for sending commands, receiving updates,
/// and handling backend events such as discovered nodes and unread messages.
pub struct Client {
    command_send: Sender<Command>,
    command_receive: Receiver<Command>,
    flood_send: Sender<ListOfDiscoveredEdgeNodes>,
    flood_recv: Receiver<ListOfDiscoveredEdgeNodes>,
    unread_msg_send: Sender<UnreadMessagesFromServer>,
    unread_msg_recv: Receiver<UnreadMessagesFromServer>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    #[must_use]
    /// Creates a new `Client` instance.
    ///
    /// Initializes all required channels for backend communication,
    /// including command, flood, and unread message streams.
    ///
    /// # Returns
    /// A fully initialized `Client` ready for use.
    pub fn new() -> Self {
        // Create channels
        //
        // API commands tot the backend
        let (command_send, command_receive) = unbounded::<Command>();
        // To get flood responses from back to front
        let (send_flood_res_channel, recv_flood_res_channel) =
            unbounded::<ListOfDiscoveredEdgeNodes>();
        // To get unread messages
        let (send_serve_unread_msg, recv_server_unread_msg) =
            unbounded::<UnreadMessagesFromServer>();

        // TODO do I need to save node-event channel here so that
        // it doesn't get dropped?
        Client {
            command_send,
            command_receive,
            flood_send: send_flood_res_channel,
            flood_recv: recv_flood_res_channel,
            unread_msg_send: send_serve_unread_msg,
            unread_msg_recv: recv_server_unread_msg,
        }
    }

    /// # Errors
    /// Starts the client's main execution loop.
    ///
    /// Spawns necessary threads and begins listening to events from the backend.
    pub fn run(&self, options: &NodeOptions, channel: &Sender<NodeEvent>) -> Result<()> {
        let mut client_backend = Service::new(
            options.id,
            channel.clone(),
            options.command_recv.clone(),
            options.packet_send.clone(),
            options.packet_recv.clone(),
            self.command_receive.clone(),
            self.flood_send.clone(),
            self.unread_msg_send.clone(),
        )
        .map_err(|e| anyhow!(e))?;

        // Move backend to different thread
        thread::spawn(move || -> Result<()> {
            client_backend.run();
            Ok(())
        });

        let server = server::start_server(
            self.command_send.clone(),
            options.id.into(),
            options.id,
            self.flood_recv.clone(),
            self.unread_msg_recv.clone(),
        );
        actix_web::rt::System::new().block_on(server)?;
        Ok(())
    }
}
