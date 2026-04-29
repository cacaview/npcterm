mod terminal;
mod input;
mod screen;
mod status;
mod manager;
mod mcp;
#[cfg(feature = "viewer")]
mod web;

use std::time::Duration;
use turbomcp::prelude::*;
use turbomcp_server::ProtocolVersion;

#[cfg(feature = "gui")]
fn run_gui_mode(cols: usize, rows: usize, shell: Option<&str>) {
    use crate::terminal::emulator::TerminalEmulator;
    use crate::terminal::gui::GuiTerminal;

    let emulator = TerminalEmulator::new(cols, rows, shell)
        .expect("Failed to create terminal emulator");

    let gui = GuiTerminal::new(cols, rows);
    gui.run(emulator);
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Parse CLI args
    let mut args = std::env::args().skip(1);
    let mut gui_mode = false;
    let mut gui_cols = 80;
    let mut gui_rows = 24;
    let mut gui_shell = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-v" => {
                println!("npcterm {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--gui" => {
                gui_mode = true;
            }
            "--size" | "-s" => {
                if let Some(size) = args.next() {
                    match size.as_str() {
                        "120x40" => { gui_cols = 120; gui_rows = 40; }
                        "160x40" => { gui_cols = 160; gui_rows = 40; }
                        "200x50" => { gui_cols = 200; gui_rows = 50; }
                        _ => { gui_cols = 80; gui_rows = 24; }
                    }
                }
            }
            "--shell" => {
                if let Some(shell) = args.next() {
                    gui_shell = Some(shell);
                }
            }
            _ => {}
        }
    }

    #[cfg(feature = "gui")]
    if gui_mode {
        run_gui_mode(gui_cols, gui_rows, gui_shell.as_deref());
        return;
    }

    #[cfg(feature = "viewer")]
    let (broadcast_tx, interactions) = {
        let (tx, _) = tokio::sync::broadcast::channel(64);
        let interactions = std::sync::Arc::new(std::sync::Mutex::new(
            web::interaction::InteractionLog::default(),
        ));
        (tx, interactions)
    };

    #[cfg(feature = "viewer")]
    let server = mcp::NpcTermServer::new_with_viewer(
        broadcast_tx.clone(),
        std::sync::Arc::clone(&interactions),
    );
    #[cfg(not(feature = "viewer"))]
    let server = mcp::NpcTermServer::new();

    // Background tick thread — processes PTY output every 10ms
    let tick_registry = server.registry_handle();
    #[cfg(feature = "viewer")]
    let tick_broadcast = broadcast_tx;

    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(10));
        if let Ok(mut reg) = tick_registry.try_lock() {
            reg.tick_all();
            #[cfg(feature = "viewer")]
            web::broadcast_updates(&mut reg, &tick_broadcast);
        }
    });

    let protocol = ProtocolConfig {
        preferred_version: ProtocolVersion::V2025_11_25,
        supported_versions: vec![
            ProtocolVersion::Unknown("2024-11-05".into()),
            ProtocolVersion::V2025_06_18,
            ProtocolVersion::V2025_11_25,
        ],
        allow_fallback: false,
    };

    server
        .builder()
        .with_protocol(protocol)
        .serve()
        .await
        .expect("MCP server error");
}
