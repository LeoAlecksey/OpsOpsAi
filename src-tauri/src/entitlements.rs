//! What this installation may use: the free plan or Pro (docs/PHASE_0_DECISIONS.md, D-13).
//!
//! Единственное место, где план превращается в набор возможностей. Интерфейс и команды
//! спрашивают у Entitlements, а не проверяют план сами — тогда License Client (Phase 3) заменит
//! только `current_plan`, а остальной код не изменится. До Phase 3 план всегда бесплатный.

use serde::Serialize;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    Free,
    Pro,
}

#[derive(Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entitlements {
    pub plan: Plan,
    /// own promo cards (courses, channels) in the sidebar; Pro hides them
    pub show_promo: bool,
    /// skills: saved presets of instructions and settings for a chat
    pub skills: bool,
    /// folders and projects as chat context
    pub folders: bool,
}

/// The capabilities of a plan. Local chat, models and history are never behind a plan.
pub fn for_plan(plan: Plan) -> Entitlements {
    match plan {
        Plan::Free => Entitlements { plan, show_promo: true, skills: false, folders: false },
        Plan::Pro => Entitlements { plan, show_promo: false, skills: true, folders: true },
    }
}

/// The plan of this installation. Licensing is Phase 3; until then everyone is on Free.
fn current_plan() -> Plan {
    Plan::Free
}

#[tauri::command]
pub fn entitlements_get() -> Entitlements {
    for_plan(current_plan())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_shows_promo_and_locks_pro_features() {
        let e = for_plan(Plan::Free);
        assert!(e.show_promo);
        assert!(!e.skills && !e.folders);
    }

    #[test]
    fn pro_hides_promo_and_unlocks_everything() {
        let e = for_plan(Plan::Pro);
        assert!(!e.show_promo);
        assert!(e.skills && e.folders);
    }

    #[test]
    fn everyone_is_on_free_until_licensing_exists() {
        assert_eq!(entitlements_get().plan, Plan::Free);
    }

    #[test]
    fn the_ui_gets_camel_case_fields() {
        let json = serde_json::to_value(for_plan(Plan::Free)).unwrap();
        assert_eq!(json["plan"], "free");
        assert_eq!(json["showPromo"], true);
    }
}
