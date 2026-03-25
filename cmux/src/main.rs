mod app;
mod model;
mod notifications;
mod session;
mod socket;
mod ui;

use tracing_subscriber::EnvFilter;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("cmux starting");

    // Run the GTK application
    let exit_code = app::run();
    std::process::exit(exit_code);
}
