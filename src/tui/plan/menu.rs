use super::state::{PlanAction, PlanMenu, PlanMenuItem};

pub fn default_plan_menu() -> PlanMenu {
    PlanMenu {
        selected: 0,
        items: vec![
            PlanMenuItem {
                label: "确认计划".to_string(),
                action: PlanAction::Confirm,
            },
            PlanMenuItem {
                label: "补充计划".to_string(),
                action: PlanAction::Revise,
            },
            PlanMenuItem {
                label: "放弃计划".to_string(),
                action: PlanAction::Abandon,
            },
        ],
    }
}
