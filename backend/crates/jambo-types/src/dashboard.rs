#[derive(Debug, Clone)]
pub struct GameFilter {
    pub statuses: Vec<String>,
    pub order_by: String,
    pub bet_min: Option<i32>,
    pub bet_max: Option<i32>,
}
