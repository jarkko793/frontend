//! Actix-web HTTP handlers for client frontend.
//!
//! This module exposes HTTP endpoints to:
//! - Serve the frontend HTML (`index`).
//! - Initiate and query network discovery via flooding (`/flood`).
//! - Register clients with servers (`/register`).
//! - Send chat messages to clients through servers (`/send`).
//! - Request list of connected clients from a server (`/clients`).
//! - Retrieve unread messages from the backend (`/messages`).
//!
//! Each endpoint interacts with the client backend via command channels,
//! forwarding commands and awaiting responses through crossbeam channels.
//! Responses are converted into appropriate HTTP status codes and JSON payloads.

use actix_files::NamedFile;
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use ap_client_backend_v2::backend::{Command, ListOfDiscoveredEdgeNodes, UnreadMessagesFromServer};
use crossbeam_channel::{Receiver, Sender, select, tick};
use messages::{ChatRequest, Message, MessageType, RequestType};
use serde::Deserialize;
use std::time::Duration;
use wg_2024::packet::NodeType;

/// Serves the main HTML file for the web frontend.
/// Called when a GET request is made to `/`
///
/// # Errors
/// Returns an error if `index.html` cannot be opened.
pub async fn index(_req: HttpRequest) -> actix_web::Result<NamedFile> {
    Ok(NamedFile::open("static/index.html")?)
}

#[get("/flood")]
/// Initiates a network flood to discover edge nodes, then retrieves the list of discovered nodes.
/// - Sends `InitializeFlood` command.
/// - Waits 2 seconds.
/// - Sends `GetEdgeNodesFromFlood` command.
/// - Filters the results to only return IDs of nodes of type `Server`.
/// Returns HTTP 500 on any backend communication failure.
pub async fn flood_network(
    command_send_channel: web::Data<Sender<Command>>,
    flood_res_channel: web::Data<Receiver<ListOfDiscoveredEdgeNodes>>,
) -> impl Responder {
    // Trigger flood initialization
    if command_send_channel.send(Command::InitializeFlood).is_err() {
        return HttpResponse::InternalServerError()
            .json("Failed to send request to the backend to flood");
    }

    // Give backend time to perform flood discovery
    std::thread::sleep(Duration::from_secs(2));

    // Request the discovered edge nodes
    if command_send_channel
        .send(Command::GetEdgeNodesFromFlood)
        .is_err()
    {
        return HttpResponse::InternalServerError()
            .json("Failed to send request to get nodes from the backend");
    }

    // Receive node list from backend
    let nodes = flood_res_channel.recv();
    match nodes {
        Ok(nodes) => {
            let mut ids = vec![];
            // Keep only nodes of type Server
            for node in nodes.0 {
                if let NodeType::Server = node.1 {
                    ids.push(node.0);
                }
            }
            HttpResponse::Ok().json(ids)
        }
        Err(_) => {
            HttpResponse::InternalServerError().json("Failed to receive answer from the backend")
        }
    }
}

#[derive(Deserialize)]
struct RegisterRequest {
    id: u8, // Target node ID to register with
}

#[post("/register")]
/// Sends a registration request to another node.
/// Constructs a `Register` chat request from the current node (`client_id`) to the target `id`.
pub async fn register(
    payload: web::Json<RegisterRequest>,
    client_id: web::Data<u8>,
    command_send_channel: web::Data<Sender<Command>>,
) -> impl Responder {
    let msg = Message {
        source: **client_id,
        destination: payload.id,
        session_id: 0,
        content: MessageType::Request(RequestType::ChatRequest(ChatRequest::Register)),
    };

    match command_send_channel.send(Command::SendMessage(msg)) {
        Ok(()) => HttpResponse::Ok(),
        Err(_) => HttpResponse::InternalServerError(),
    }
}

#[derive(Deserialize)]
struct SendRequest {
    server_id: u8,   // ID of the server to send message through
    client_id: u8,   // Target client ID to send message to
    message: String, // Message content
}

#[post("/send")]
/// Sends a chat message from this node to a target client through a server.
/// Builds a `SendMessage` chat request and forwards it to the backend.
pub async fn send_message(
    payload: web::Json<SendRequest>,
    node_id: web::Data<u8>,
    command_send_channel: web::Data<Sender<Command>>,
) -> impl Responder {
    let msg = Message {
        source: *node_id.get_ref(),
        destination: payload.server_id,
        session_id: 0,
        content: MessageType::Request(RequestType::ChatRequest(ChatRequest::SendMessage {
            from: *node_id.get_ref(),
            to: payload.client_id,
            message: payload.message.clone(),
        })),
    };

    match command_send_channel.send(Command::SendMessage(msg)) {
        Ok(()) => HttpResponse::Ok(),
        Err(_) => HttpResponse::InternalServerError(),
    }
}

#[post("/clients")]
/// Requests a list of connected clients from a server.
/// Sends a `ClientList` chat request to the target server.
pub async fn clients(
    payload: web::Json<SendRequest>,
    node_id: web::Data<u8>,
    command_send_channel: web::Data<Sender<Command>>,
) -> impl Responder {
    let msg = Message {
        source: *node_id.get_ref(),
        destination: payload.server_id,
        session_id: 0,
        content: MessageType::Request(RequestType::ChatRequest(ChatRequest::ClientList)),
    };

    match command_send_channel.send(Command::SendMessage(msg)) {
        Ok(()) => HttpResponse::Ok(),
        Err(_) => HttpResponse::InternalServerError(),
    }
}

#[get("/messages")]
/// Retrieves unread messages from the backend.
/// - Sends `GetUnreadMessagesFromServer` command.
/// - Waits up to 3 seconds for a response using a channel select.
/// - Returns messages if available, otherwise HTTP 204 (No Content).
pub async fn get_messages(
    cmd_channel: web::Data<Sender<Command>>,
    unread_msg_channel: web::Data<Receiver<UnreadMessagesFromServer>>,
) -> impl Responder {
    let res = cmd_channel.send(Command::GetUnreadMessagesFromServer);

    match res {
        Ok(()) => {
            let timeout_tick = tick(Duration::from_secs(3));

            // Wait for either messages or timeout
            select! {
                recv(unread_msg_channel) -> msg => match msg {
                    Ok(msgs) => {
                        if msgs.0.is_empty(){
                            HttpResponse::NoContent().json("No new messages")
                        } else {
                            HttpResponse::Ok().json(msgs.0)
                        }
                    },
                    _ => { HttpResponse::Ok().json("No new messages") },
                },
                recv(timeout_tick) -> _ => {
                    HttpResponse::NoContent().json("No new messages")
                }
            }
        }
        Err(_) => HttpResponse::InternalServerError().json("Failed to send request to the backend"),
    }
}
