// pub mod validate;
pub mod validate_simple;
// pub mod evaluate;
pub mod evaluate_simple;
// pub mod resolve;
pub mod branch_ops;
pub mod branch_ops_v2;
pub mod expand;
pub mod merge;
pub mod pool_resolution;
pub mod solve_pipeline;
pub mod instance_filter;

// pub use validate::*;
pub use validate_simple::*;
// pub use evaluate::*;
pub use evaluate_simple::*;
// pub use resolve::*;
// Re-export only from branch_ops_v2 (the newer version)
// pub use branch_ops::*;  // Old version - commented out to avoid conflicts
pub use branch_ops_v2::*;
pub use expand::*;
pub use merge::*;
pub use pool_resolution::*;
pub use solve_pipeline::{SolvePipeline, SolvePipelineWithStore};
pub use instance_filter::*;
