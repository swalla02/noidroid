//! The Stand.
//!
//! Araki names Stands after the music he likes — Killer Queen, Echoes, Crazy Diamond,
//! Highway Star, and Radiohead's own Creep. PARANOID ANDROID is built to that rule,
//! so anyone who has read JoJo will know what it is on sight; anyone who has not sees
//! a tool that records executions and never has to think about it again.
//!
//! `noidroid stand` is the only place the reference is load-bearing, and nothing in
//! the workflow goes through it. The six parameters are the joke and the documentation
//! at once: they are graded honestly, so the stat block is an accurate summary of what
//! this thing can and cannot do.

use crate::palette::{bold, dim, frame};
use crate::palette::{
    ink, provenance, shell, AMBER, ASH, CHROME, CRIMSON, CYAN, INDIGO, PHOSPHOR, VIOLET,
};

/// A Stand parameter: the grade, and why it is that grade.
struct Parameter {
    name: &'static str,
    grade: char,
    because: &'static str,
}

/// Clockwise from the top, as Araki draws them.
const PARAMETERS: [Parameter; 6] = [
    Parameter {
        name: "DESTRUCTIVE POWER",
        grade: 'E',
        because: "it cannot change what happened, and that is the point",
    },
    Parameter {
        name: "SPEED",
        grade: 'C',
        because: "returning to step k costs re-running steps 0..k; no snapshot fast-path yet",
    },
    Parameter {
        name: "RANGE",
        grade: 'B',
        because: "one machine, one process, the boundaries it was told about — browsers included",
    },
    Parameter {
        name: "PERSISTENCE",
        grade: 'A',
        because: "trajectories are immutable and content-addressed; they outlive the execution",
    },
    Parameter {
        name: "PRECISION",
        grade: 'A',
        because: "a reconstruction is verified by hash equality, or it is reported as failed",
    },
    Parameter {
        name: "DEVELOPMENTAL POTENTIAL",
        grade: 'A',
        because: "the core knows nothing about any environment; adapters are protocol clients",
    },
];

fn grade_colour(grade: char) -> String {
    let text = grade.to_string();
    match grade {
        'A' => ink(PHOSPHOR, &text),
        'B' => ink(CYAN, &text),
        'C' => ink(CHROME, &text),
        'D' => ink(AMBER, &text),
        _ => ink(ASH, &text),
    }
}

/// The menacing glyphs. Every JoJo panel that matters has them somewhere.
pub const MENACE: &str = "ゴ ゴ ゴ ゴ";

pub fn print_profile() {
    let g: Vec<String> = PARAMETERS.iter().map(|p| grade_colour(p.grade)).collect();

    println!();
    println!("        {}", dim(MENACE));
    println!("   {}", shell("「 P A R A N O I D   A N D R O I D 」"));
    println!("        {}", dim(MENACE));
    println!();
    println!("   {:<22}{}", dim("STAND NAME"), bold("PARANOID ANDROID"));
    println!(
        "   {:<22}Radiohead — \"Paranoid Android\" (1997)",
        dim("NAMESAKE")
    );
    println!(
        "   {:<22}whoever is holding the terminal",
        dim("STAND USER")
    );
    println!(
        "   {:<22}Automatic · bound to an execution rather than to a body",
        dim("STAND TYPE")
    );
    println!();
    println!("   {}", dim("ABILITY"));
    println!(
        "   {}",
        bold("An execution it has witnessed can be returned to and continued differently.")
    );
    println!("   The original is never touched.");
    println!();

    // The stat hexagon, clockwise from the top, the way Araki prints it.
    //
    // Built from one fixed-width template with positional holes. Colouring a single
    // character does not change how wide it renders, so the geometry cannot drift
    // the way it does when each line is assembled by hand.
    let core = ink(VIOLET, "◈");
    let name_top = ink(INDIGO, "PARANOID");
    let name_bottom = ink(INDIGO, "ANDROID");
    // Each line is its own literal, so the leading columns are exactly as written.
    // A backslash continuation would eat them, and colouring one character does not
    // change how wide it renders, so the geometry holds.
    let hex = format!(
        concat!(
            "                     {label_top}\n",
            "                             {top}\n",
            "                     {rule_top}\n",
            "   {dev_label} {dev} ────{left}       {core}       {right}── {speed} {speed_label}\n",
            "     POTENTIAL       {bar}   {name_top}    {bar}\n",
            "                     {bar}    {name_bottom}    {bar}\n",
            "     {prec_label} {prec} ────{left}               {right}── {range} {range_label}\n",
            "                     {rule_bottom}\n",
            "                             {bottom}\n",
            "                        {label_bottom}",
        ),
        label_top = dim("DESTRUCTIVE POWER"),
        top = g[0],
        rule_top = frame("╱───────────────╲"),
        dev_label = dim("DEVELOPMENT"),
        dev = g[5],
        left = frame("┤"),
        core = core,
        right = frame("├"),
        speed = g[1],
        speed_label = dim("SPEED"),
        bar = frame("│"),
        name_top = name_top,
        name_bottom = name_bottom,
        prec_label = dim("PRECISION"),
        prec = g[4],
        range = g[2],
        range_label = dim("RANGE"),
        rule_bottom = frame("╲───────────────╱"),
        bottom = g[3],
        label_bottom = dim("PERSISTENCE"),
    );
    println!("{hex}");
    println!();

    // The grades are graded honestly, so say why each one is what it is.
    for (parameter, coloured) in PARAMETERS.iter().zip(&g) {
        println!(
            "   {} {:<25} {}",
            coloured,
            dim(parameter.name),
            parameter.because
        );
    }

    println!();
    println!(
        "   {}",
        dim("THE COLOURWAY — and what each colour means here")
    );
    for (name, label) in [
        ("real", "real"),
        ("live", "live"),
        ("simulated", "simulated"),
        ("unknown", "unknown"),
    ] {
        let meaning = match name {
            "real" => "observed in the execution that actually happened",
            "live" => "really executed, but in a counterfactual world",
            "simulated" => "supplied by an intervention; nobody ran it",
            _ => "needed, and not available — the edge of what can be said",
        };
        println!("   {:>12}   {}", provenance(label), dim(meaning));
    }
    println!();
    println!(
        "   {} {}",
        ink(CRIMSON, "▁▁▁"),
        dim("divergence · refusal · an ability that did not work")
    );
    println!();
    println!("   {}", bold("EXPLORE FROM HERE."));
    println!("        {}", dim(MENACE));
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_parameter_is_graded_and_justified() {
        assert_eq!(PARAMETERS.len(), 6, "a Stand has six parameters");
        for p in &PARAMETERS {
            assert!(
                "ABCDE".contains(p.grade),
                "{} has grade {:?}, which is not on Araki's scale",
                p.name,
                p.grade
            );
            assert!(
                !p.because.is_empty(),
                "{} needs to say why it is graded that way; the stat block doubles as \
                 the capability summary",
                p.name
            );
        }
    }

    #[test]
    fn destructive_power_stays_at_e() {
        // If this ever rises, the tool has started changing what happened, which is
        // the one thing it must never do.
        let power = PARAMETERS
            .iter()
            .find(|p| p.name == "DESTRUCTIVE POWER")
            .expect("the first parameter is destructive power");
        assert_eq!(power.grade, 'E');
    }
}
