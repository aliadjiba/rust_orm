use std::sync::OnceLock;
use std::collections::HashMap;

static IRREGULAR: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static IRREGULAR_REVERSE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

fn irregular_map() -> &'static HashMap<&'static str, &'static str> {
    IRREGULAR.get_or_init(|| {
        HashMap::from([
            ("person", "people"),
            ("child", "children"),
            ("mouse", "mice"),
            ("man", "men"),
            ("woman", "women"),
            ("analysis", "analyses"),
            ("news", "news"),
            ("series", "series"),
            ("species", "species"),

        ])
    })
}

fn irregular_reverse() -> &'static HashMap<&'static str, &'static str> {
    IRREGULAR_REVERSE.get_or_init(|| {
        HashMap::from([
            ("people", "person"),
            ("children", "child"),
            ("mice", "mouse"),
            ("men", "man"),
            ("women", "woman"),
            ("analyses", "analysis"),
        ])
    })
}

fn plural_score(name: &str) -> f32 {
    let lower = name.to_lowercase();

    // 1. STRICT IRREGULAR CHECK (highest confidence)
    if irregular_map().contains_key(lower.as_str()) {
        return 0.0; // singular form
    }

    if irregular_reverse().contains_key(lower.as_str()) {
        return 1.0; // plural form
    }

    let mut score: f32 = 0.0;

    // 2. strong plural endings
    if lower.ends_with("ies") {
        score += 0.9;
    }

    if lower.ends_with("es") {
        score += 0.6;
    }

    if lower.ends_with('s') {
        score += 0.4;
    }

    // 's' preceded by a consonant (e.g. "ingredients", "cats", "dogs")
    // but NOT "ss", "us", "is" which are already handled
    if lower.ends_with('s') && lower.len() > 3 {
        let prev = lower.as_bytes()[lower.len() - 2];
        if prev != b's' && prev != b'u' && prev != b'i' {
            score += 0.4;
        }
    }

    // 3. strong singular indicators
    if lower.ends_with("ss") {
        score -= 0.5;
    }

    if lower.ends_with("us") || lower.ends_with("is") {
        score -= 0.3;
    }

    // 4. invariant nouns (news, series, species)
    if matches!(lower.as_str(), "news" | "series" | "species") {
        return 0.5; // neutral (neither singular nor plural strongly)
    }

    // 5. normalization
    if lower.len() <= 3 {
        score -= 0.5;
    }

    score.clamp(0.0, 1.0)
}

pub fn is_plural(name: &str) -> bool { plural_score(name) > 0.7 }

pub fn pluralize(name: &str) -> String {
    let lower = name.to_lowercase();

    let irregular = irregular_map();

    // 1. forward irregular lookup (including invariant nouns like "news")
    if let Some(plural) = irregular.get(lower.as_str()) {
        return plural.to_string();
    }

    // 2. y → ies (company → companies)
    if lower.ends_with("y")
        && !lower.ends_with("ay")
        && !lower.ends_with("ey")
        && !lower.ends_with("oy")
        && !lower.ends_with("uy")
    {
        return format!("{}ies", &lower[..lower.len() - 1]);
    }

    // 3. s / x / z / ch / sh → es
    if lower.ends_with("s")
        || lower.ends_with("x")
        || lower.ends_with("z")
        || lower.ends_with("ch")
        || lower.ends_with("sh")
    {
        return format!("{}es", lower);
    }

    // 4. default rule
    format!("{}s", lower)
}

pub fn singularize(name: &str) -> String {
    let lower = name.to_lowercase();
    let irregular = irregular_map();

    // reverse lookup (plural → singular)
    if let Some((singular, _)) = irregular.iter().find(|(_, v)| *v == &lower) {
        return singular.to_string();
    }

    if lower.ends_with("ies") {
        return format!("{}y", &lower[..lower.len() - 3]);
    }

    // Only strip "es" if the stem ends in s, x, z, ch, or sh
    if lower.ends_with("es") {
        let stem = &lower[..lower.len() - 2];
        if stem.ends_with('s')
            || stem.ends_with('x')
            || stem.ends_with('z')
            || stem.ends_with("ch")
            || stem.ends_with("sh")
        {
            return stem.to_string();
        }
        // otherwise fall through to the 's' rule
    }

    if lower.ends_with('s') {
        return lower[..lower.len() - 1].to_string();
    }

    lower
}