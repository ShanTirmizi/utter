//! The names people in tech say all day: services, tools, languages and
//! jargon. A small speech model hears "Kubernetes" as "cooper netties" and
//! writes "type script" for TypeScript, and the user should not have to add
//! every one of them to Your words. So these come built in, used the way the
//! user's own words are, for that one dictation:
//!
//! - the most distinctive go to the speech model as hints, after the user's
//!   own words and the names on screen;
//! - any that sounds like part of the dictation is named to the cleanup
//!   model, with how it is spelled;
//! - a few spoken forms that can only mean one thing are written right
//!   straight away, so they are right even with cleanup off.

use crate::words::{Source, Word};

/// Each name as its makers write it, and what a speech model writes instead
/// (lower case). Names that are also everyday words (Go, Rust, Swift, Linear,
/// Notion, Claude) are here for cleanup, which decides from the sentence;
/// they are never hints to the speech model, which would then hear them
/// everywhere.
const NAMES: &[(&str, &[&str])] = &[
    // AI
    ("Anthropic", &["entropic", "and tropic", "anthropics"]),
    ("Claude", &["cloud", "clawed", "claud", "clod"]),
    ("Claude Code", &["cloud code", "clawed code"]),
    ("Haiku", &["high cool", "hi coo"]),
    ("Sonnet", &[]),
    ("Opus", &[]),
    ("OpenAI", &["open ai", "open a i", "open eye"]),
    ("ChatGPT", &["chat gpt", "chat g p t", "chad gpt"]),
    ("GPT", &["g p t"]),
    ("Gemini", &[]),
    // Both real, and said the same: the sentence decides, never this list.
    ("Groq", &[]),
    ("Grok", &[]),
    ("Mistral", &[]),
    ("Llama", &["lama"]),
    ("Ollama", &["o llama", "olama"]),
    ("Hugging Face", &["hugging face"]),
    ("LangChain", &["lang chain"]),
    ("LlamaIndex", &["llama index"]),
    ("Perplexity", &[]),
    ("Cursor", &[]),
    ("Copilot", &["co pilot"]),
    ("Whisper", &[]),
    ("ElevenLabs", &["eleven labs", "11 labs", "11labs"]),
    ("Midjourney", &["mid journey"]),
    ("LLM", &["l l m", "llm"]),
    ("LLMs", &["l l ms", "llms"]),
    ("RAG", &["rag"]),
    ("MCP", &["m c p", "mcp"]),
    ("evals", &["e-vals", "e vals", "evils", "e-val"]),
    ("fine-tuning", &["fine tuning"]),
    ("embeddings", &[]),
    ("PyTorch", &["pie torch", "pi torch", "py torch"]),
    ("TensorFlow", &["tensor flow"]),
    // Cloud and infrastructure
    ("AWS", &["a w s", "aws"]),
    ("Azure", &["asure", "azur"]),
    ("GCP", &["g c p", "gcp"]),
    ("Kubernetes", &["cooper netties", "kuber netties", "cubernetes", "cooper nettis", "kubernetties"]),
    ("kubectl", &["cube control", "kube control", "cube cuttle", "kube cuttle"]),
    ("Docker", &[]),
    ("Terraform", &["terra form"]),
    ("Ansible", &[]),
    ("Vercel", &["versel", "vercell", "versal", "versailles"]),
    ("Netlify", &["net lify", "netlifly"]),
    ("Cloudflare", &["cloud flare"]),
    ("Supabase", &["super base", "supa base", "superbase"]),
    ("Firebase", &["fire base"]),
    ("Heroku", &["hero ku", "heroku"]),
    ("DigitalOcean", &["digital ocean"]),
    ("Nginx", &["engine x", "engine-x", "n jinx"]),
    ("Kafka", &[]),
    ("Redis", &["reddis", "red is", "radis"]),
    ("PostgreSQL", &["postgres sequel", "post gress sequel", "postgre sql"]),
    ("Postgres", &["post gres", "post gress", "postgress"]),
    ("MySQL", &["my sequel", "my sql"]),
    ("SQLite", &["sequel lite", "sql lite", "sequelite"]),
    ("MongoDB", &["mongo db", "mongo d b"]),
    ("DynamoDB", &["dynamo db"]),
    ("Snowflake", &[]),
    ("Databricks", &["data bricks"]),
    ("Datadog", &["data dog"]),
    ("Sentry", &[]),
    ("Grafana", &["grafanna"]),
    ("Prometheus", &[]),
    ("CI/CD", &["ci cd", "c i c d", "ci/cd"]),
    ("DevOps", &["dev ops"]),
    // Code and the web
    ("GitHub", &["git hub", "get hub", "github"]),
    ("GitLab", &["git lab", "get lab"]),
    ("Bitbucket", &["bit bucket"]),
    ("TypeScript", &["type script", "typescript"]),
    ("JavaScript", &["java script", "javascript"]),
    ("Python", &[]),
    ("Rust", &[]),
    ("Go", &[]),
    ("Swift", &[]),
    ("Kotlin", &["cotlin"]),
    ("React", &[]),
    ("React Native", &[]),
    ("Next.js", &["next js", "next jay ess", "nextjs", "next.js"]),
    ("Node.js", &["node js", "nodejs", "node.js"]),
    ("Vue", &["view js"]),
    ("Svelte", &["svelt", "s velte"]),
    ("Tailwind", &["tail wind"]),
    ("Vite", &["veet", "vight"]),
    ("Tauri", &["tory", "tauri", "towery"]),
    ("Electron", &[]),
    ("Django", &["jango"]),
    ("FastAPI", &["fast api", "fast a p i"]),
    ("GraphQL", &["graph ql", "graph q l", "graphql"]),
    ("gRPC", &["g rpc", "grpc"]),
    ("REST API", &["rest api"]),
    ("API", &["a p i", "api"]),
    ("APIs", &["a p is", "apis"]),
    ("SDK", &["s d k", "sdk"]),
    ("CLI", &["c l i", "cli"]),
    ("JSON", &["jason", "json"]),
    ("YAML", &["yammel", "yamel", "yaml"]),
    ("OAuth", &["o auth", "oh auth"]),
    ("JWT", &["j w t", "jwt"]),
    ("npm", &["n p m", "npm"]),
    ("pnpm", &["p n p m", "pnpm"]),
    ("useEffect", &["use effect", "user effect", "user-fect", "usefect"]),
    ("useState", &["use state", "user state"]),
    ("localhost", &["local host"]),
    ("Xcode", &["x code", "ex code"]),
    ("VS Code", &["vs code", "v s code", "vscode"]),
    ("Stack Overflow", &["stack overflow"]),
    ("staging", &[]),
    ("prod", &[]),
    ("dev", &[]),
    ("PR", &["p r"]),
    ("PRs", &["p rs"]),
    ("QA", &["q a", "queue a"]),
    ("macOS", &["mac os", "macos"]),
    ("iOS", &["ios"]),
    ("iPhone", &["iphone"]),
    // Work tools
    ("Slack", &[]),
    ("Notion", &[]),
    ("Linear", &[]),
    ("Jira", &["jeera", "gira"]),
    ("Confluence", &[]),
    ("Figma", &["fig ma", "figmo"]),
    ("Stripe", &[]),
    ("Twilio", &["twillio", "twilio"]),
    ("Shopify", &[]),
    ("Salesforce", &["sales force"]),
    ("HubSpot", &["hub spot"]),
    ("Zapier", &["zappier", "zap ear"]),
    ("Loom", &[]),
    ("Calendly", &["calendley"]),
];

/// Names too much like everyday words to give the speech model as hints:
/// with "Claude" in its prompt it hears every "cloud" as Claude.
const NOT_HINTS: &[&str] = &[
    "Claude", "Claude Code", "Haiku", "Sonnet", "Opus", "Gemini", "Grok", "Llama", "Cursor", "Whisper",
    "Perplexity", "embeddings", "Docker", "Snowflake", "Sentry", "Prometheus", "Python", "Rust", "Go",
    "Swift", "React", "React Native", "Vue", "Electron", "staging", "prod", "dev", "PR", "PRs", "QA",
    "Slack", "Notion", "Linear", "Confluence", "Stripe", "Shopify", "Loom", "API", "APIs", "Kafka",
    "fine-tuning", "evals", "iPhone",
];

/// How many of these go to the speech model as hints, at most. Its prompt
/// is short, and the user's own words and the names on screen come first.
const MOST_HINTS: usize = 40;

/// Is `heard` already one of these names, however it is cased or spaced?
/// "Postgres" is a name in its own right, not a mishearing of PostgreSQL.
pub fn is_a_name(heard: &str) -> bool {
    let squash = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let heard = squash(heard);
    NAMES.iter().any(|(name, _)| squash(name) == heard)
}

/// The built-in names as words for one dictation. Never saved.
pub fn words() -> Vec<Word> {
    NAMES
        .iter()
        .map(|(name, heard)| Word {
            word: name.to_string(),
            heard_as: heard.iter().map(|h| h.to_string()).collect(),
            source: Source::Builtin,
            added_at: 0,
            on_screen: false,
        })
        .collect()
}

/// Add the built-in names to a dictation's words, after the user's own: a
/// name the user has their own spelling for keeps theirs.
pub fn add_to(words: &mut Vec<Word>) {
    let known: std::collections::HashSet<String> = words.iter().map(|w| w.word.to_lowercase()).collect();
    words.extend(self::words().into_iter().filter(|w| !known.contains(&w.word.to_lowercase())));
}

/// The most distinctive names, as hints for the speech model.
pub fn hints() -> Vec<String> {
    NAMES
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !NOT_HINTS.contains(name))
        .take(MOST_HINTS)
        .map(str::to_string)
        .collect()
}

/// Spoken forms that can only mean one name, written as that name: "type
/// script" is TypeScript, "next js" Next.js, "git hub" GitHub. Whole words
/// only, any case. Not for code editors and terminals, where a command such
/// as `npm` or `kubectl` is typed exactly as said.
const ONLY_ONE_MEANING: &[(&str, &str)] = &[
    ("type script", "TypeScript"),
    ("typescript", "TypeScript"),
    ("java script", "JavaScript"),
    ("javascript", "JavaScript"),
    ("git hub", "GitHub"),
    ("github", "GitHub"),
    ("git lab", "GitLab"),
    ("gitlab", "GitLab"),
    ("open ai", "OpenAI"),
    ("openai", "OpenAI"),
    ("chat gpt", "ChatGPT"),
    ("chatgpt", "ChatGPT"),
    ("next js", "Next.js"),
    ("nextjs", "Next.js"),
    ("node js", "Node.js"),
    ("nodejs", "Node.js"),
    ("graph ql", "GraphQL"),
    ("graphql", "GraphQL"),
    ("mongo db", "MongoDB"),
    ("mongodb", "MongoDB"),
    ("dynamo db", "DynamoDB"),
    ("postgresql", "PostgreSQL"),
    ("mysql", "MySQL"),
    ("eleven labs", "ElevenLabs"),
    ("elevenlabs", "ElevenLabs"),
    ("hugging face", "Hugging Face"),
    ("super base", "Supabase"),
    ("supabase", "Supabase"),
    ("cloud flare", "Cloudflare"),
    ("cloudflare", "Cloudflare"),
    ("kubernetes", "Kubernetes"),
    ("terraform", "Terraform"),
    ("tailwind", "Tailwind"),
    ("dev ops", "DevOps"),
    ("devops", "DevOps"),
    ("ci cd", "CI/CD"),
    ("ci/cd", "CI/CD"),
    ("macos", "macOS"),
    ("mac os", "macOS"),
    ("xcode", "Xcode"),
    ("vs code", "VS Code"),
    ("vscode", "VS Code"),
    ("pytorch", "PyTorch"),
    ("tensorflow", "TensorFlow"),
    ("fastapi", "FastAPI"),
    ("langchain", "LangChain"),
    ("api", "API"),
    ("apis", "APIs"),
    ("sdk", "SDK"),
    ("sdks", "SDKs"),
    ("json", "JSON"),
    ("yaml", "YAML"),
    ("llm", "LLM"),
    ("llms", "LLMs"),
    ("aws", "AWS"),
    ("gcp", "GCP"),
    ("oauth", "OAuth"),
    ("jwt", "JWT"),
    ("html", "HTML"),
    ("css", "CSS"),
    ("url", "URL"),
    ("urls", "URLs"),
    ("ui", "UI"),
    ("ux", "UX"),
    ("http", "HTTP"),
    ("https", "HTTPS"),
    ("sql", "SQL"),
    ("gpu", "GPU"),
    ("gpus", "GPUs"),
    ("cpu", "CPU"),
    ("ios", "iOS"),
    ("iphone", "iPhone"),
    ("ipad", "iPad"),
];

/// Write each spoken form in `ONLY_ONE_MEANING` as its name.
pub fn fix_spoken(text: &str) -> String {
    let mut text = text.to_string();
    for (said, name) in ONLY_ONE_MEANING {
        text = replace_phrase(&text, said, name);
    }
    text
}

/// Replace whole-word, case-insensitive matches of `said` (one or more
/// words, single spaces) with `name`. Words are runs of letters and digits;
/// "api's" contains "api", "rapid" does not.
fn replace_phrase(text: &str, said: &str, name: &str) -> String {
    let lower = text.to_lowercase();
    // Lower-casing can change byte lengths outside ASCII; only then would
    // positions not line up, so such text is left alone.
    if lower.len() != text.len() {
        return text.to_string();
    }
    let is_word = |c: char| c.is_alphanumeric();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(said) {
        let start = at + found;
        let end = start + said.len();
        let before = lower[..start].chars().next_back();
        let after = lower[end..].chars().next();
        // A dot or slash on either side is part of something else: a file
        // name (api.ts), a path or a web address.
        let joined = |c: Option<char>| c.is_some_and(|c| is_word(c) || c == '.' || c == '/' || c == '_' || c == '-');
        let ends_sentence = after == Some('.') && lower[end + 1..].chars().next().is_none_or(char::is_whitespace);
        if joined(before) || (joined(after) && !ends_sentence) {
            out.push_str(&text[at..end]);
        } else {
            out.push_str(&text[at..start]);
            out.push_str(name);
        }
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spoken_forms_with_one_meaning_are_written_as_the_name() {
        assert_eq!(fix_spoken("add a type script test and push it to git hub"), "add a TypeScript test and push it to GitHub");
        assert_eq!(fix_spoken("Deploy the next js app with the open ai api."), "Deploy the Next.js app with the OpenAI API.");
        assert_eq!(fix_spoken("Store it as json, not yaml"), "Store it as JSON, not YAML");
        assert_eq!(fix_spoken("we run ci cd on aws"), "we run CI/CD on AWS");
    }

    #[test]
    fn parts_of_other_words_names_and_paths_are_left_alone() {
        assert_eq!(fix_spoken("a rapid build"), "a rapid build");
        assert_eq!(fix_spoken("open api.ts and config.json"), "open api.ts and config.json");
        assert_eq!(fix_spoken("see github.com/acme"), "see github.com/acme");
        assert_eq!(fix_spoken("the guide is fine"), "the guide is fine");
        // Text whose lower case is a different length is left as it is.
        assert_eq!(fix_spoken("İstanbul api"), "İstanbul api");
        assert_eq!(fix_spoken("Ελληνικά api"), "Ελληνικά API");
    }

    #[test]
    fn everyday_names_are_never_hints_for_the_speech_model() {
        let hints = hints();
        assert!(hints.len() <= MOST_HINTS);
        assert!(hints.iter().any(|h| h == "Kubernetes"));
        for everyday in ["Claude", "Go", "Rust", "Slack", "Notion"] {
            assert!(!hints.iter().any(|h| h == everyday), "{everyday}");
        }
    }

    #[test]
    fn a_word_that_is_already_a_name_is_not_a_mishearing() {
        assert!(is_a_name("postgres") && is_a_name("Groq") && is_a_name("next.js"));
        assert!(!is_a_name("superbase") && !is_a_name("versel"));
    }

    #[test]
    fn every_name_is_listed_once() {
        let mut names: Vec<&str> = NAMES.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }
}
