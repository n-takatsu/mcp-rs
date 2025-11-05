pub mod app;
pub mod charts;
pub mod events;
pub mod runner;
pub mod ui;

pub use app::DashboardApp;
pub use charts::{EventLog, MetricsChart, TrafficChart};
pub use events::{DashboardEvent, EventHandler};
pub use runner::run_dashboard;
pub use ui::render_dashboard;
