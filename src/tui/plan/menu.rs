use crate::i18n::text_key;

use super::state::{PlanAction, PlanMenu, PlanMenuItem};

pub fn default_plan_menu() -> PlanMenu {
    PlanMenu {
        selected: 0,
        items: vec![
            PlanMenuItem {
                label: text_key("plan.confirm"),
                action: PlanAction::Confirm,
            },
            PlanMenuItem {
                label: text_key("plan.revise"),
                action: PlanAction::Revise,
            },
            PlanMenuItem {
                label: text_key("plan.abandon"),
                action: PlanAction::Abandon,
            },
        ],
    }
}
