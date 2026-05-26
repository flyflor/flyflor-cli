#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanMenu {
    pub selected: usize,
    pub items: Vec<PlanMenuItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanAction {
    Confirm,
    Revise,
    Abandon,
}

impl PlanAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Revise => "revise",
            Self::Abandon => "abandon",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanMenuItem {
    pub label: String,
    pub action: PlanAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanPendingAction {
    Revise,
}
