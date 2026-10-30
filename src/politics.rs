//! Source-driven political-power ledger; no country-specific starting balances.
//! Numeric source data are imported. Native-game differential parity is separate.
use crate::{
    campaign::CountryPolitics,
    pdx::{self, Entry},
    research::ResearchCatalogue,
    session::GameSession,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoliticalRuntime {
    pub last_day: BTreeMap<String, u64>,
    pub income: BTreeMap<String, PowerIncome>,
    pub failures: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerIncome {
    pub base: f64,
    pub flat: f64,
    pub factor: f64,
    pub cost: f64,
    pub net: f64,
    pub stability_factor: f64,
    pub focus_gain: f64,
    pub focus_factor: f64,
    pub focus_cost: f64,
}
fn define(cat: &ResearchCatalogue, key: &str) -> Result<f64, String> {
    cat.scripts
        .defines
        .get(key)
        .copied()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("Missing/invalid define {key}"))
}
pub fn limits(cat: &ResearchCatalogue) -> Result<(f64, f64), String> {
    let min = define(cat, "NDefines.NCountry.POLITICAL_POWER_LOWER_CAP")?;
    let max = define(cat, "NDefines.NCountry.POLITICAL_POWER_UPPER_CAP")?;
    if min > max {
        return Err("Invalid political power caps".into());
    }
    Ok((min, max))
}
/// Scalar modifiers from native common/modifiers. Hidden numeric modifiers are
/// still modifiers. Metadata blocks are not mistaken for country modifiers.
pub fn static_value(cat: &ResearchCatalogue, id: &str, key: &str) -> Result<f64, String> {
    fn read(b: &[Entry], key: &str) -> Result<f64, String> {
        let mut value = 0.;
        for e in b {
            if e.key == key {
                if e.operator != "=" {
                    return Err(format!("Invalid modifier operator {}", e.key));
                }
                value += e
                    .atom()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| format!("Nonliteral static modifier {key}"))?;
            } else if e.key == "hidden_modifier" {
                value += read(e.block().ok_or("hidden_modifier requires block")?, key)?;
            }
        }
        if !value.is_finite() {
            return Err("Static modifier overflow".into());
        }
        Ok(value)
    }
    let raw = cat
        .scripts
        .static_modifiers
        .get(id)
        .ok_or_else(|| format!("Missing static modifier {id}"))?;
    read(raw, key)
}
pub fn history(b: &[Entry], cat: &ResearchCatalogue) -> Result<CountryPolitics, String> {
    // HOI4 initializes the PP account at zero, then executes history effects.
    // Stability/war support defaults belong to imported defines, not constants.
    let fraction = |key: &str| {
        cat.scripts
            .defines
            .get(key)
            .copied()
            .filter(|v| v.is_finite() && (0.0..=1.).contains(v))
    };
    let mut p = CountryPolitics {
        power: Some(0.),
        stability: fraction("NDefines.NCountry.DEFAULT_STABILITY"),
        war_support: fraction("NDefines.NCountry.DEFAULT_WAR_SUPPORT"),
    };
    for e in b {
        let field = match e.key.as_str() {
            "set_political_power" | "add_political_power" => &mut p.power,
            "set_stability" | "add_stability" => &mut p.stability,
            "set_war_support" | "add_war_support" => &mut p.war_support,
            _ => continue,
        };
        let n = e
            .atom()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .ok_or_else(|| {
                format!(
                    "Unknown history expression {} at line {}",
                    e.key, e.span.line
                )
            })?;
        let value = if e.key.starts_with("set_") {
            Some(n)
        } else {
            field.map(|old| old + n)
        };
        if value.is_some_and(|v| !v.is_finite()) {
            return Err("Political history overflow".into());
        }
        *field = if e.key.contains("political_power") {
            value
        } else {
            value.map(|v| v.clamp(0., 1.))
        };
    }
    Ok(p)
}
/// Reuse the existing ordered dated-history resolver with real selected DLCs.
/// Unresolved selectors must not turn a conditional PP award into a fake zero.
pub fn resolved_history(
    b: &[Entry],
    tag: &str,
    date: crate::game::GameDate,
    cat: &ResearchCatalogue,
    context: &crate::research::ResearchContext,
) -> Result<CountryPolitics, String> {
    fn has_dlc(b: &[Entry]) -> bool {
        b.iter()
            .any(|e| e.key == "has_dlc" || e.block().is_some_and(has_dlc))
    }
    if context.dlc_selection_unverified && has_dlc(b) {
        return Err("Political history requires a verified DLC selection".into());
    }
    let initial = crate::military_model::MilitaryModifiers {
        dlcs: context.dlcs.clone(),
        ideas: cat.scripts.default_laws.values().cloned().collect(),
        ..Default::default()
    };
    let (entries, warnings) =
        crate::history_conditions::resolve_expanding(b, tag, &initial, date, &|e| match cat
            .scripts
            .effects
            .get(&e.key)
        {
            Some(raw) => {
                if context.dlc_selection_unverified && has_dlc(raw) {
                    return Err("Scripted political history requires verified DLC selection".into());
                }
                hoi4_mobile::script_validation::instantiate(raw, e).map(Some)
            }
            None => Ok(None),
        });
    if !warnings.is_empty() {
        return Err(format!(
            "Unresolved political history: {}",
            warnings.join("; ")
        ));
    }
    history(&entries, cat)
}
impl GameSession {
    pub fn political_income(
        &self,
        tag: &str,
        cat: &ResearchCatalogue,
    ) -> Result<PowerIncome, String> {
        if let Some(error) = self
            .campaign
            .dynamic_modifiers
            .get(tag)
            .into_iter()
            .flat_map(|m| m.values())
            .find_map(|m| m.blocked.as_ref())
        {
            return Err(format!("Dynamic modifier blocked: {error}"));
        }
        let country = self
            .research
            .country(tag)
            .ok_or("Country research state missing")?;
        fn unresolved_dynamic(b: &[Entry]) -> bool {
            b.iter().any(|e| {
                matches!(
                    e.key.as_str(),
                    "add_dynamic_modifier" | "remove_dynamic_modifier"
                ) || e.block().is_some_and(unresolved_dynamic)
            })
        }
        if country
            .pending_effects
            .iter()
            .any(|e| unresolved_dynamic(&e.payload))
        {
            return Err(
                "Historical dynamic modifier has not been executed; income is unverified".into(),
            );
        }
        let p = self
            .campaign
            .countries
            .get(tag)
            .ok_or("Country political state missing")?;
        let get = |key: &str| -> Result<f64, String> {
            let n = country.modifier(key)
                + self
                    .military
                    .countries
                    .get(tag)
                    .and_then(|c| c.base_modifiers.get(key))
                    .copied()
                    .unwrap_or(0.);
            if !n.is_finite() {
                return Err(format!("Non-finite modifier {key}"));
            }
            Ok(n)
        };
        let base = define(cat, "NDefines.NPolitics.BASE_POLITICAL_POWER_INCREASE")?;
        let stability = p
            .stability
            .ok_or("Stability unknown for political income")?;
        let weight = (stability - 0.5).abs() * 2.;
        let stability_factor = if weight > 0. {
            static_value(
                cat,
                if stability > 0.5 {
                    "stability_good_modifier"
                } else {
                    "stability_bad_modifier"
                },
                "political_power_factor",
            )? * weight
        } else {
            0.
        };
        let focus = self.campaign.focuses.active.contains_key(tag);
        let focus_gain = if focus {
            static_value(cat, "gain_focus", "political_power_gain")?
        } else {
            0.
        };
        let focus_factor = if focus {
            static_value(cat, "gain_focus", "political_power_factor")?
        } else {
            0.
        };
        let focus_cost = if focus {
            static_value(cat, "gain_focus", "political_power_cost")?
        } else {
            0.
        };
        let flat = get("political_power_gain")? + focus_gain;
        let factor = get("political_power_factor")? + stability_factor + focus_factor;
        let cost = get("political_power_cost")? + focus_cost;
        let net = (base + flat) * (1. + factor) - cost;
        if !net.is_finite() {
            return Err("Political income overflow".into());
        }
        limits(cat)?;
        Ok(PowerIncome {
            base,
            flat,
            factor,
            cost,
            net,
            stability_factor,
            focus_gain,
            focus_factor,
            focus_cost,
        })
    }
    pub fn tick_politics(&mut self, cat: &ResearchCatalogue) {
        let day = self.clock.elapsed_hours / 24;
        let tags: Vec<_> = self
            .world
            .countries
            .keys()
            .filter(|t| !self.campaign.inactive.contains(*t))
            .cloned()
            .collect();
        for tag in tags {
            if self
                .campaign
                .politics
                .last_day
                .get(&tag)
                .is_some_and(|old| *old >= day)
            {
                continue;
            }
            // Record the attempted day as well: fixing data later must not
            // backfill the wrong day's balance or repeat a previous payment.
            self.campaign.politics.last_day.insert(tag.clone(), day);
            let result = (|| -> Result<PowerIncome, String> {
                let income = self.political_income(&tag, cat)?;
                let (min, max) = limits(cat)?;
                let p = self
                    .campaign
                    .countries
                    .get_mut(&tag)
                    .ok_or("Political state missing")?;
                let next = p.power.ok_or("Political power unknown")? + income.net;
                if !next.is_finite() {
                    return Err("Political balance overflow".into());
                }
                p.power = Some(next.clamp(min, max));
                Ok(income)
            })();
            match result {
                Ok(income) => {
                    self.campaign.politics.income.insert(tag.clone(), income);
                    self.campaign.politics.failures.remove(&tag);
                }
                Err(e) => {
                    self.campaign.politics.income.remove(&tag);
                    self.campaign.politics.failures.insert(tag, e);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Display {
    pub tag: String,
    pub values: CountryPolitics,
    pub income: Result<PowerIncome, String>,
}
impl GameSession {
    pub fn politics_display(&self, cat: &ResearchCatalogue) -> Option<Display> {
        let tag = self.player_country.as_ref()?;
        Some(Display {
            tag: tag.clone(),
            values: self
                .campaign
                .countries
                .get(tag)
                .cloned()
                .unwrap_or_default(),
            income: self.political_income(tag, cat),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn prepared() -> (GameSession, ResearchCatalogue) {
        let (mut s, mut cat, _) = crate::focus::tests::prepared();
        cat.scripts.defines.insert(
            "NDefines.NPolitics.BASE_POLITICAL_POWER_INCREASE".into(),
            2.,
        );
        for (id, factor) in [
            ("stability_good_modifier", 0.2),
            ("stability_bad_modifier", -0.4),
        ] {
            cat.scripts.static_modifiers.insert(
                id.into(),
                pdx::parse_pdx(&format!("political_power_factor={factor}")).unwrap(),
            );
        }
        s.clock.elapsed_hours = 24;
        (s, cat)
    }
    #[test]
    fn source_defaults_and_ordered_history_override_without_guessing_stability() {
        let (_, mut cat) = prepared();
        cat.scripts
            .defines
            .insert("NDefines.NCountry.DEFAULT_STABILITY".into(), 0.63);
        cat.scripts
            .defines
            .insert("NDefines.NCountry.DEFAULT_WAR_SUPPORT".into(), 0.27);
        let p=history(&pdx::parse_pdx("add_political_power=12 set_political_power=80 add_political_power=-5 add_stability=0.1").unwrap(),&cat).unwrap();
        assert_eq!(p.power, Some(75.));
        assert!((p.stability.unwrap() - 0.73).abs() < 1e-9);
        assert_eq!(p.war_support, Some(0.27));
        assert!(
            history(
                &pdx::parse_pdx("set_political_power=var:missing").unwrap(),
                &cat
            )
            .is_err()
        );
        cat.scripts
            .defines
            .remove("NDefines.NCountry.DEFAULT_STABILITY");
        assert!(history(&[], &cat).unwrap().stability.is_none());
    }
    #[test]
    fn daily_ledger_caps_idempotence_and_save_resume() {
        let (mut s, cat) = prepared();
        s.tick_politics(&cat);
        assert_eq!(s.campaign.countries["A"].power, Some(102.));
        s.tick_politics(&cat);
        assert_eq!(s.campaign.countries["A"].power, Some(102.));
        s.campaign = serde_json::from_slice(&serde_json::to_vec(&s.campaign).unwrap()).unwrap();
        s.tick_politics(&cat);
        assert_eq!(s.campaign.countries["A"].power, Some(102.));
        s.clock.elapsed_hours += 24;
        s.campaign.countries.get_mut("A").unwrap().power = Some(1999.);
        s.tick_politics(&cat);
        assert_eq!(s.campaign.countries["A"].power, Some(2000.));
        s.research
            .countries
            .get_mut("A")
            .unwrap()
            .source_modifiers
            .insert(
                "test".into(),
                BTreeMap::from([("political_power_cost".into(), 3000.)]),
            );
        s.clock.elapsed_hours += 24;
        s.tick_politics(&cat);
        assert_eq!(s.campaign.countries["A"].power, Some(-500.));
    }
    #[test]
    fn focus_and_idea_costs_are_one_daily_transaction_not_double_debit() {
        let (mut s, mut cat) = prepared();
        let rules = crate::research::ResearchRules::default();
        s.campaign.countries.get_mut("A").unwrap().stability = Some(0.75);
        s.research
            .countries
            .get_mut("A")
            .unwrap()
            .source_modifiers
            .insert(
                "test".into(),
                BTreeMap::from([
                    ("political_power_gain".into(), 0.4),
                    ("political_power_factor".into(), 0.2),
                    ("political_power_cost".into(), 0.3),
                ]),
            );
        cat.scripts.static_modifiers.insert("gain_focus".into(),pdx::parse_pdx("political_power_gain=-1 hidden_modifier={political_power_factor=0.1} political_power_cost=0.5").unwrap());
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        let income = s.political_income("A", &cat).unwrap();
        assert!((income.net - 1.16).abs() < 1e-9);
        s.tick_politics(&cat);
        s.tick_focuses(&cat, &rules);
        assert!((s.campaign.countries["A"].power.unwrap() - 101.16).abs() < 1e-9);
        assert_eq!(s.campaign.focuses.active["A"].progress, 1.);
    }
    #[test]
    fn missing_source_rules_do_not_create_free_progress_or_fake_income() {
        let (mut s, mut cat) = prepared();
        let rules = Default::default();
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        cat.scripts
            .defines
            .remove("NDefines.NPolitics.BASE_POLITICAL_POWER_INCREASE");
        s.tick_politics(&cat);
        s.tick_focuses(&cat, &rules);
        assert_eq!(s.campaign.countries["A"].power, Some(100.));
        assert_eq!(s.campaign.focuses.active["A"].progress, 0.);
        assert!(s.campaign.politics.failures["A"].contains("define"));
    }
    #[test]
    fn conditional_starting_power_uses_selected_dlc_and_unknown_guards_stay_unknown() {
        let (_, mut cat) = prepared();
        let mut ctx = crate::research::ResearchContext::default();
        ctx.dlcs.insert("Test".into());
        let b = pdx::parse_pdx(
            "if={limit={has_dlc=Test} add_political_power=40} else={add_political_power=7}",
        )
        .unwrap();
        assert_eq!(
            resolved_history(&b, "A", crate::game::GameDate::START, &cat, &ctx)
                .unwrap()
                .power,
            Some(40.)
        );
        ctx.dlcs.clear();
        assert_eq!(
            resolved_history(&b, "A", crate::game::GameDate::START, &cat, &ctx)
                .unwrap()
                .power,
            Some(7.)
        );
        ctx.dlc_selection_unverified = true;
        assert!(resolved_history(&b, "A", crate::game::GameDate::START, &cat, &ctx).is_err());
        cat.scripts.effects.insert("startup".into(), b.clone());
        assert!(
            resolved_history(
                &pdx::parse_pdx("startup=yes").unwrap(),
                "A",
                crate::game::GameDate::START,
                &cat,
                &ctx
            )
            .is_err()
        );
        assert!(
            resolved_history(
                &pdx::parse_pdx("if={limit={missing_start_fact=yes} add_political_power=40}")
                    .unwrap(),
                "A",
                crate::game::GameDate::START,
                &cat,
                &Default::default()
            )
            .is_err()
        );
    }
}
