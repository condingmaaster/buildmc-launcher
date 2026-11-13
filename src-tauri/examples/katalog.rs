//! Zeigt den Fragenkatalog so, wie er im Code steht.
use buildmc_launcher_lib::assistant::questions::{catalogue, evaluate, Context};

fn main() {
    let ctx = Context { versions: vec!["1.21.9".into(), "1.21.4".into()], system_ram_mb: 16384 };

    for (i, q) in catalogue(&ctx).iter().enumerate() {
        println!("\n\x1b[1m{}. {}\x1b[0m", i + 1, q.prompt);
        println!("   \x1b[2m{}{}\x1b[0m", q.why,
                 if q.multi { "  (Mehrfachauswahl)" } else { "" });
        for o in &q.options {
            let cats = if o.categories.is_empty() { String::new() }
                       else { format!(" #{}", o.categories.join(",")) };
            let such = if o.queries.is_empty() && cats.is_empty() { String::new() }
                       else { format!("   \x1b[2m→ sucht: {}{}{}\x1b[0m",
                              o.queries.join(", "), cats,
                              o.project_type.as_ref().map(|t| format!(" [{t}]")).unwrap_or_default()) };
            println!("     ○ {:<28} {}", o.label, o.detail);
            if !such.is_empty() { println!("       {such}"); }
        }
    }

    println!("\n\x1b[1m── Beispiel: schwacher Rechner, Technik, Shader, Minikarte ──\x1b[0m");
    let antworten = vec![
        ("ziel".to_string(), vec!["technik".to_string()]),
        ("geraet".to_string(), vec!["schwach".to_string()]),
        ("optik".to_string(), vec!["shader".to_string()]),
        ("komfort".to_string(), vec!["karte".to_string(), "rezepte".to_string()]),
        ("server".to_string(), vec!["ja".to_string()]),
    ];
    let o = evaluate(&antworten);
    println!("  Optimierungsstufe: {:?}", o.level.unwrap());
    println!("  Bildlauf:          {:?}", o.pacing.unwrap());
    println!("  Suchen:");
    for (kind, query, categories) in &o.searches {
        let bezeichnung = if query.is_empty() { categories.join(",") } else { query.clone() };
        println!("    [{kind:<12}] {bezeichnung}");
    }
    for n in &o.notes {
        println!("  \x1b[33mHinweis:\x1b[0m {n}");
    }
}
