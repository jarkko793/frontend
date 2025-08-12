/// Public module `endpoints` containing HTTP handlers for various API routes.
pub mod endpoints;

use actix_web::App;
use actix_web::HttpServer;
use actix_web::web;
use ap_client_backend_v2::backend::Command;
use ap_client_backend_v2::backend::ListOfDiscoveredEdgeNodes;
use ap_client_backend_v2::backend::UnreadMessagesFromServer;
use crossbeam_channel::{Receiver, Sender};
use endpoints::clients;
use endpoints::flood_network;
use endpoints::get_messages;
use endpoints::index;
use endpoints::register;
use endpoints::send_message;

/// Starts the Actix Web HTTP server for the client API.
///
/// The server exposes endpoints for:
/// - Registering nodes
/// - Sending messages
/// - Retrieving messages
/// - Discovering nearby nodes
/// - Viewing connected clients
///
/// # Arguments
/// * `command_send_channel` - Channel used to send backend commands.
/// * `port` - The base port number. The server will bind to `port + 8000`.
/// * `node_id` - Unique identifier for the local node.
/// * `flood_recv_channel` - Channel for receiving lists of discovered edge nodes.
/// * `unread_msg_recv_channel` - Channel for receiving unread messages from the backend.
///
/// # Returns
/// An [`std::io::Result`] which is `Ok(())` if the server started successfully.
///
/// # Errors
/// Returns an [`std::io::Error`] if binding to the port or starting the server fails.
pub async fn start_server(
    command_send_channel: Sender<Command>,
    port: u16,
    node_id: u8,
    flood_recv_channel: Receiver<ListOfDiscoveredEdgeNodes>,
    unread_msg_recv_channel: Receiver<UnreadMessagesFromServer>,
) -> std::io::Result<()> {
    let port = port + 8000;
    HttpServer::new(move || {
        App::new()
            .service(clients)
            .service(register)
            .service(send_message)
            .service(get_messages)
            .service(flood_network)
            .route("/", web::get().to(index))
            .app_data(web::Data::new(command_send_channel.clone()))
            .app_data(web::Data::new(flood_recv_channel.clone()))
            .app_data(web::Data::new(unread_msg_recv_channel.clone()))
            .app_data(web::Data::new(node_id))
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}
