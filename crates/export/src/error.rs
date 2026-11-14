//! Why an order package could not be built or written.
//!
//! Every failure is a value the command line can print: a document whose geometry does not resolve,
//! a placement that is not a rigid transform, a table whose record does not match its columns, or
//! the operating system refusing a file. The exporter never panics and never writes half a table.

use furnicraft_model::{ContourSegmentError, PanelContourError, PanelGeometryError, PlacementError};
use thiserror::Error;

/// Why an order package could not be built or written.
#[derive(Debug, Error)]
pub enum ExportError {
    /// The panel's nominal outline does not resolve to a ring.
    #[error("panel outline: {0}")]
    Contour(#[from] PanelContourError),
    /// A contour segment cannot be read back as a point or a curve.
    #[error("contour segment: {0}")]
    Segment(#[from] ContourSegmentError),
    /// The panel's core and overall regions do not resolve.
    #[error("panel geometry: {0}")]
    Geometry(#[from] PanelGeometryError),
    /// A placement cannot be composed or inverted.
    #[error("placement: {0}")]
    Placement(#[from] PlacementError),
    /// A record does not carry one value per declared column.
    #[error("table `{table}` record has {got} values but {expected} columns")]
    ColumnCount {
        /// The table whose record was rejected.
        table: &'static str,
        /// The number of values the record carried.
        got: usize,
        /// The number of columns the table declares.
        expected: usize,
    },
    /// A value does not have the type its column declares.
    #[error("table `{table}` column `{column}` expects {expected}, got {got}")]
    ColumnType {
        /// The table whose record was rejected.
        table: &'static str,
        /// The column whose type did not match.
        column: &'static str,
        /// The declared type.
        expected: &'static str,
        /// The type of the value the row builder passed.
        got: &'static str,
    },
    /// The package builder tried to fill a table the column table does not declare.
    #[error("the package declares no table `{0}`")]
    MissingTable(&'static str),
    /// The operating system refused a file or a directory.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// A manifest or a table could not be serialized.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// The system clock is before the Unix epoch, so no export time can be written.
    #[error("the system clock is before the Unix epoch")]
    Clock,
}
