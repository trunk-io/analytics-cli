#![cfg_attr(docsrs, feature(doc_cfg))]

//! # Rust types and traits definitions to implement Bazel's Build event protcol.
//!
//! To learn what does what check out [bazel's docs](https://bazel.build/remote/bep)

pub use prost_types;

#[cfg(feature = "client")]
#[cfg_attr(docsrs, doc(cfg(feature = "client")))]
pub mod client {
    pub use super::google::devtools::build::v1::publish_build_event_client::*;
}

#[cfg(feature = "server")]
#[cfg_attr(docsrs, doc(cfg(feature = "server")))]
pub mod server {
    pub use super::google::devtools::build::v1::publish_build_event_server::*;
}

pub mod types {
    pub mod build_event_stream {
        pub use crate::build_event_stream::*;
    }
    pub mod blaze {
        pub use crate::blaze::*;
    }
    pub mod command_line {
        pub use crate::command_line::*;
    }
    pub mod failure_details {
        pub use crate::failure_details::*;
    }
    pub mod options {
        pub use crate::options::*;
    }
    pub mod package_metrics {
        pub use crate::package_metrics::*;
    }
    pub mod google {
        pub use crate::google::*;
    }
    pub mod devtools {
        pub use crate::devtools::*;
    }
}

pub(crate) mod build_event_stream {
    include!(concat!(env!("OUT_DIR"), "/build_event_stream.rs"));
    include!(concat!(env!("OUT_DIR"), "/build_event_stream.serde.rs"));
}

pub(crate) mod blaze {
    include!(concat!(env!("OUT_DIR"), "/blaze.rs"));
    include!(concat!(env!("OUT_DIR"), "/blaze.serde.rs"));
    pub use invocation_policy::*;
    pub mod invocation_policy {
        include!(concat!(env!("OUT_DIR"), "/blaze.invocation_policy.rs"));
        include!(concat!(
            env!("OUT_DIR"),
            "/blaze.invocation_policy.serde.rs"
        ));
    }
}

pub(crate) mod command_line {
    include!(concat!(env!("OUT_DIR"), "/command_line.rs"));
    include!(concat!(env!("OUT_DIR"), "/command_line.serde.rs"));
}

pub(crate) mod failure_details {
    include!(concat!(env!("OUT_DIR"), "/failure_details.rs"));
    include!(concat!(env!("OUT_DIR"), "/failure_details.serde.rs"));
}

pub(crate) mod options {
    include!(concat!(env!("OUT_DIR"), "/options.rs"));
    include!(concat!(env!("OUT_DIR"), "/options.serde.rs"));
}

pub(crate) mod package_metrics {
    pub use crate::devtools::*;
}

pub(crate) mod google {
    pub use devtools::*;
    pub mod devtools {
        pub use build::*;
        pub mod build {
            pub use v1::*;
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/google.devtools.build.v1.rs"));
            }
        }
    }
    pub use api::*;
    pub mod api {
        include!(concat!(env!("OUT_DIR"), "/google.api.rs"));
    }
}

pub(crate) mod devtools {
    pub use build::*;
    pub mod build {
        pub use lib::*;
        pub mod lib {
            pub use packages::*;
            pub mod packages {
                pub use metrics::*;
                pub mod metrics {
                    include!(concat!(
                        env!("OUT_DIR"),
                        "/devtools.build.lib.packages.metrics.rs"
                    ));
                    include!(concat!(
                        env!("OUT_DIR"),
                        "/devtools.build.lib.packages.metrics.serde.rs"
                    ));
                }
            }
        }
    }
}
