use crate::house::*;
use serde::{Deserialize, Serialize};

/// A mailing is a collection of people with mailing addresses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Mailing {
    pub house: House,
}
