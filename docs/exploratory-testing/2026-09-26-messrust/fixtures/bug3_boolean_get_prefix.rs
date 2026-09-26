//! Reproducer for Bug 3: BooleanGetMethodName falsely flags non-getter methods whose names start with "get".

pub struct Task;

impl Task {
    // Control: true boolean getters that should be flagged
    pub fn get_ready(&self) -> bool {
        true
    }

    pub fn get_status(&self) -> bool {
        true
    }

    // Bug: Ordinary methods whose English word happens to start with "get", but are NOT getters
    pub fn getting_started(&self) -> bool {
        true
    }

    pub fn getter(&self) -> bool {
        true
    }

    pub fn gets_updated(&self) -> bool {
        true
    }
}
