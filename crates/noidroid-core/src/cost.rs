//! What a trajectory bought.
//!
//! A recorded model call carries the provider's own `usage` block, so token counts
//! are a recorded fact: they are read back out of the response object, never
//! estimated. Whether those tokens were *bought* is a second recorded fact, and a
//! different one — it is the step's [`Delivery`], which says whether this run
//! executed the call or served it off the tape. A branch that shares its parent's
//! model call spends nothing, and that is the sentence worth printing.
//!
//! A price is not a recorded fact. Nothing here has a default price list, because a
//! default price list is a made-up number wearing a fact's clothes, and this is the
//! project that refuses those. [`Price`] only ever comes from the caller. Money is
//! reported when a price was supplied, and when zero tokens were bought — zero costs
//! nothing at every price there is.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::error::Result;
use crate::model::{Action, Delivery, Trajectory};
use crate::repo::Repo;

/// The delivery key for a step this run kept no note of. Not a [`Delivery`] label:
/// a bundle carries content, not per-run notes, so an imported trajectory genuinely
/// does not say whether its calls were executed. Saying so beats guessing.
pub const UNRECORDED: &str = "unrecorded";

/// Tokens a provider reported for one call, in its own vocabulary.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    /// Every other `*_tokens` counter the provider reported, under the provider's own
    /// name for it. Anthropic's cache counters land here. They are never folded into
    /// `input`: they are billed at a different rate, and averaging them in would be
    /// inventing a price.
    pub extra: BTreeMap<String, u64>,
}

impl Usage {
    pub fn is_zero(&self) -> bool {
        self.input == 0 && self.output == 0 && self.extra.values().all(|n| *n == 0)
    }

    pub fn add(&mut self, other: &Usage) {
        self.input += other.input;
        self.output += other.output;
        for (name, count) in &other.extra {
            *self.extra.entry(name.clone()).or_insert(0) += count;
        }
    }
}

/// One model's usage under one delivery.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Tally {
    pub calls: u64,
    pub usage: Usage,
}

impl Tally {
    pub fn add(&mut self, usage: &Usage) {
        self.calls += 1;
        self.usage.add(usage);
    }
}

/// What one model cost under one delivery, in one trajectory.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    /// The provider's name for the model that answered.
    pub model: String,
    /// [`Delivery::label`], or [`UNRECORDED`].
    pub delivery: &'static str,
    pub tally: Tally,
}

impl Entry {
    /// Only an executed call reaches a provider, and only a call that reaches a
    /// provider is billed.
    pub fn spent(&self) -> bool {
        self.delivery == Delivery::Executed.label()
    }
}

/// Every model call in a trajectory, split by model and by how this run got it.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Ledger {
    pub trajectory: String,
    /// Sorted by model, then delivery.
    pub entries: Vec<Entry>,
    /// Calls to a model whose recorded response carries no usage block we can read —
    /// a streamed response, say. Counted so that they are visibly missing rather than
    /// silently totalled as zero.
    pub calls_without_usage: u64,
}

impl Ledger {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.calls_without_usage == 0
    }

    /// What this run actually bought.
    pub fn spent(&self) -> Tally {
        self.fold(|e| e.spent())
    }

    /// What it used and demonstrably did not buy: served from the recording,
    /// supplied by an intervention, or denied. Tokens whose delivery nothing recorded
    /// are excluded — they belong to [`Ledger::undeliverable`], because calling them
    /// "not spent" would be the same guess as calling them spent.
    pub fn not_spent(&self) -> Tally {
        self.fold(|e| !e.spent() && e.delivery != UNRECORDED)
    }

    /// Tokens whose delivery this run kept no note of. Nonzero means we cannot say
    /// whether they were bought, and must not claim either way.
    pub fn undeliverable(&self) -> Tally {
        self.fold(|e| e.delivery == UNRECORDED)
    }

    /// Call counts by delivery, most calls first, then alphabetically.
    pub fn by_delivery(&self) -> Vec<(&'static str, Tally)> {
        let mut totals: BTreeMap<&'static str, Tally> = BTreeMap::new();
        for entry in &self.entries {
            let slot = totals.entry(entry.delivery).or_default();
            slot.calls += entry.tally.calls;
            slot.usage.add(&entry.tally.usage);
        }
        let mut out: Vec<(&'static str, Tally)> = totals.into_iter().collect();
        out.sort_by(|a, b| b.1.calls.cmp(&a.1.calls).then(a.0.cmp(b.0)));
        out
    }

    /// What each model was bought for, models with no executed call omitted.
    pub fn spent_by_model(&self) -> Vec<(&str, Usage)> {
        let mut totals: BTreeMap<&str, Usage> = BTreeMap::new();
        for entry in self.entries.iter().filter(|e| e.spent()) {
            totals
                .entry(entry.model.as_str())
                .or_default()
                .add(&entry.tally.usage);
        }
        totals.into_iter().collect()
    }

    /// Every model this trajectory used, bought or not.
    pub fn models(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.entries.iter().map(|e| e.model.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    fn fold(&self, keep: impl Fn(&Entry) -> bool) -> Tally {
        let mut out = Tally::default();
        for entry in self.entries.iter().filter(|e| keep(e)) {
            out.calls += entry.tally.calls;
            out.usage.add(&entry.tally.usage);
        }
        out
    }

    fn add(&mut self, model: &str, delivery: &'static str, usage: &Usage) {
        match self
            .entries
            .iter_mut()
            .find(|e| e.model == model && e.delivery == delivery)
        {
            Some(entry) => entry.tally.add(usage),
            None => {
                let mut tally = Tally::default();
                tally.add(usage);
                self.entries.push(Entry {
                    model: model.to_string(),
                    delivery,
                    tally,
                });
            }
        }
    }
}

/// Add up the model calls in one trajectory.
pub fn account(repo: &Repo, t: &Trajectory) -> Result<Ledger> {
    let delivered: BTreeMap<u64, Delivery> = repo
        .load_notes(&t.name)?
        .into_iter()
        .map(|note| (note.index, note.delivery))
        .collect();

    let mut ledger = Ledger {
        trajectory: t.name.clone(),
        ..Ledger::default()
    };
    for (_, step) in repo.chain(t)? {
        let Action::Call { target, args, .. } = &step.action else {
            continue;
        };
        let delivery = match delivered.get(&step.index) {
            Some(d) => d.label(),
            None => UNRECORDED,
        };
        let mut accounted = false;
        for effect in &step.effects {
            let value: Value = repo.store.get_json(&effect.value)?;
            let Some(reported) = reported_in(&value) else {
                continue;
            };
            accounted = true;
            let model = reported
                .model
                .or_else(|| named_model(args))
                .unwrap_or_else(|| "unnamed model".to_string());
            ledger.add(&model, delivery, &reported.usage);
        }
        // A call the adapter routed to a model, whose response says nothing about
        // what it cost. Reporting it as zero would be the quiet lie.
        if !accounted && target.starts_with("model.") {
            ledger.calls_without_usage += 1;
        }
    }
    ledger
        .entries
        .sort_by(|a, b| a.model.cmp(&b.model).then(a.delivery.cmp(b.delivery)));
    Ok(ledger)
}

/// What a provider said about one call it answered.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Reported {
    pub usage: Usage,
    pub model: Option<String>,
}

/// Read a provider's own account of a call out of a recorded response.
///
/// Two shapes, both of which this repository records today: the response object as
/// the model adapter stores it, and the HTTP exchange the recording proxy stores,
/// whose body is that same object still in a string.
pub fn reported_in(value: &Value) -> Option<Reported> {
    if let Some(reported) = direct(value) {
        return Some(reported);
    }
    let body = value.as_object()?.get("body")?.as_str()?;
    let parsed: Value = serde_json::from_str(body).ok()?;
    direct(&parsed)
}

fn direct(value: &Value) -> Option<Reported> {
    let object = value.as_object()?;
    let usage = match object.get("usage") {
        Some(Value::Object(usage)) => usage,
        _ => return None,
    };
    Some(Reported {
        usage: read_usage(usage),
        model: named_model(value),
    })
}

/// `input_tokens`/`output_tokens` (Anthropic) or `prompt_tokens`/`completion_tokens`
/// (OpenAI); everything else that counts tokens is kept under its own name.
fn read_usage(usage: &Map<String, Value>) -> Usage {
    let mut out = Usage::default();
    for (name, value) in usage {
        let Some(count) = value.as_u64() else {
            continue;
        };
        match name.as_str() {
            "input_tokens" | "prompt_tokens" => out.input += count,
            "output_tokens" | "completion_tokens" => out.output += count,
            other if other.ends_with("_tokens") => {
                *out.extra.entry(other.to_string()).or_insert(0) += count;
            }
            _ => {}
        }
    }
    out
}

fn named_model(value: &Value) -> Option<String> {
    value
        .get("model")?
        .as_str()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// What a model charges, in US dollars per million tokens.
///
/// Supplied by whoever runs the command and by nobody else. There is no table of
/// defaults here on purpose: prices change, differ by contract and by region, and a
/// figure this tool made up would be indistinguishable in the output from one it
/// measured.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Price {
    pub input: f64,
    pub output: f64,
}

impl Price {
    /// `IN/OUT`, dollars per million tokens. `None` for anything that is not two
    /// non-negative numbers, so a typo is refused rather than silently priced at nil.
    pub fn parse(spec: &str) -> Option<Price> {
        let (input, output) = spec.split_once('/')?;
        let input = sane(input)?;
        let output = sane(output)?;
        Some(Price { input, output })
    }

    /// The bill for `usage`, covering input and output only. Anything in
    /// [`Usage::extra`] is billed at a rate nobody gave us, so it is left out and the
    /// caller is expected to say so.
    pub fn of(&self, usage: &Usage) -> f64 {
        (usage.input as f64 * self.input + usage.output as f64 * self.output) / 1_000_000.0
    }
}

fn sane(number: &str) -> Option<f64> {
    let value: f64 = number.trim().parse().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

/// Dollars, at enough decimal places that a real spend is never rounded into
/// looking like nothing.
pub fn dollars(amount: f64) -> String {
    let places = if amount == 0.0 || amount >= 0.01 {
        2
    } else if amount >= 0.0001 {
        4
    } else {
        6
    };
    format!("${amount:.places$}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn token_counts_are_read_from_the_response_not_estimated() {
        let anthropic = json!({
            "model": "claude-x",
            "usage": { "input_tokens": 180, "output_tokens": 24 },
        });
        let reported = reported_in(&anthropic).expect("an Anthropic-shaped response");
        assert_eq!(reported.model.as_deref(), Some("claude-x"));
        assert_eq!(reported.usage.input, 180);
        assert_eq!(reported.usage.output, 24);

        let openai = json!({
            "model": "gpt-x",
            "usage": { "prompt_tokens": 7, "completion_tokens": 3 },
        });
        let reported = reported_in(&openai).expect("an OpenAI-shaped response");
        assert_eq!(reported.usage.input, 7);
        assert_eq!(reported.usage.output, 3);

        assert_eq!(reported_in(&json!({ "model": "m" })), None);
        assert_eq!(reported_in(&json!("just a string")), None);
    }

    #[test]
    fn a_proxied_call_is_accounted_from_the_body_it_recorded() {
        // What `noidroid run --proxy` stores: an HTTP exchange, body still a string.
        let exchange = json!({
            "status": 200,
            "headers": { "content-type": "application/json" },
            "body": r#"{"model":"claude-y","usage":{"input_tokens":9,"output_tokens":4}}"#,
        });
        let reported = reported_in(&exchange).expect("the body carries the usage");
        assert_eq!(reported.model.as_deref(), Some("claude-y"));
        assert_eq!(reported.usage.input, 9);

        // A body that is not a model response is not a model call.
        let other = json!({ "status": 200, "body": "not json at all" });
        assert_eq!(reported_in(&other), None);
    }

    #[test]
    fn a_token_counter_we_do_not_price_is_reported_rather_than_folded_in() {
        // Cache tokens are billed at their own rate. Adding them to `input` would
        // price them at the input rate, which is a number nobody gave us.
        let response = json!({
            "usage": {
                "input_tokens": 10,
                "output_tokens": 2,
                "cache_read_input_tokens": 4096,
                "service_tier": "standard",
            },
        });
        let usage = reported_in(&response).unwrap().usage;
        assert_eq!(usage.input, 10);
        assert_eq!(usage.extra.get("cache_read_input_tokens"), Some(&4096));
        assert!(!usage.extra.contains_key("service_tier"));
    }

    #[test]
    fn a_price_is_two_numbers_or_it_is_refused() {
        assert_eq!(
            Price::parse("3/15"),
            Some(Price {
                input: 3.0,
                output: 15.0
            })
        );
        assert_eq!(
            Price::parse("0/0"),
            Some(Price {
                input: 0.0,
                output: 0.0
            })
        );
        assert_eq!(Price::parse("free"), None);
        assert_eq!(Price::parse("3"), None);
        assert_eq!(Price::parse("-3/15"), None);
        assert_eq!(Price::parse("3/nan"), None);
    }

    #[test]
    fn a_real_spend_is_never_rounded_into_looking_like_nothing() {
        let price = Price::parse("3/15").unwrap();
        let usage = Usage {
            input: 180,
            output: 24,
            extra: BTreeMap::new(),
        };
        assert_eq!(dollars(price.of(&usage)), "$0.0009");
        assert_eq!(dollars(price.of(&Usage::default())), "$0.00");
        assert_eq!(dollars(4.2), "$4.20");
        // Two decimal places would print this as nothing, which it is not.
        assert_eq!(dollars(0.000_003), "$0.000003");
    }

    #[test]
    fn only_an_executed_call_counts_as_bought() {
        let mut ledger = Ledger::default();
        let usage = Usage {
            input: 100,
            output: 10,
            extra: BTreeMap::new(),
        };
        ledger.add("m", Delivery::Executed.label(), &usage);
        ledger.add("m", Delivery::Replayed.label(), &usage);
        ledger.add("m", Delivery::Intervened.label(), &usage);
        ledger.add("m", UNRECORDED, &usage);

        assert_eq!(ledger.spent().calls, 1);
        assert_eq!(ledger.spent().usage.input, 100);
        assert_eq!(ledger.not_spent().calls, 2);
        assert_eq!(ledger.not_spent().usage.input, 200);
        assert_eq!(ledger.undeliverable().calls, 1);
        assert_eq!(ledger.spent_by_model(), vec![("m", usage)]);
        assert_eq!(ledger.models(), vec!["m"]);
    }
}
