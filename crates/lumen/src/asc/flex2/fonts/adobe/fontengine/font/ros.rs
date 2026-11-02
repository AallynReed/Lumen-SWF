//! Port of afe.jar `com.adobe.fontengine.font.ROS`.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ROS {
    pub registry: String,
    pub ordering: String,
    pub supplement: i32,
}

impl ROS {
    pub fn new(registry: String, ordering: String, supplement: i32) -> ROS {
        ROS { registry, ordering, supplement }
    }

    pub fn to_java_string(&self) -> String {
        format!("{} {} {}", self.registry, self.ordering, self.supplement)
    }
}
