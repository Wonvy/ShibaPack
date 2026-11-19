use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct WikiSearchResponse {
    query: WikiQuery,
}

#[derive(Deserialize)]
struct WikiQuery {
    search: Vec<WikiSearchItem>,
}

#[derive(Deserialize)]
struct WikiSearchItem {
    title: String,
}

#[derive(Deserialize)]
struct WikiExtractResponse {
    query: WikiExtractQuery,
}

#[derive(Deserialize)]
struct WikiExtractQuery {
    pages: HashMap<String, WikiExtractPage>,
}

#[derive(Deserialize)]
struct WikiExtractPage {
    extract: Option<String>,
}

#[derive(Deserialize)]
struct GithubSearchResponse {
    items: Vec<GithubRepo>,
}

#[derive(Deserialize)]
struct GithubRepo {
    description: Option<String>,
    html_url: String,
}

/// 返回 (摘要, 来源, 链接)
pub async fn lookup(name: &str) -> Option<(String, String, String)> {
    let client = Client::builder()
        .user_agent(concat!("ShibaPack/", env!("CARGO_PKG_VERSION"), " (+local package manager)"))
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .ok()?;

    // 中文维基优先（转简体），保证摘要可读；找不到再查英文维基。
    for lang in ["zh", "en"] {
        if let Some(found) = wikipedia(&client, lang, name).await {
            return Some(found);
        }
    }

    github(&client, name).await
}

async fn wikipedia(client: &Client, lang: &str, name: &str) -> Option<(String, String, String)> {
    let search_url = format!(
        "https://{lang}.wikipedia.org/w/api.php?action=query&list=search&srsearch={}&srlimit=1&format=json",
        urlencoding::encode(name)
    );
    let response = client.get(&search_url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let data: WikiSearchResponse = response.json().await.ok()?;
    let title = data.query.search.first()?.title.clone();

    let mut extract_url = reqwest::Url::parse(&format!("https://{lang}.wikipedia.org/w/api.php")).ok()?;
    {
        let mut query = extract_url.query_pairs_mut();
        query.append_pair("action", "query");
        query.append_pair("prop", "extracts");
        query.append_pair("exintro", "1");
        query.append_pair("explaintext", "1");
        query.append_pair("titles", &title);
        query.append_pair("format", "json");
        if lang == "zh" {
            query.append_pair("variant", "zh-cn");
        }
    }
    let response = client.get(extract_url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let data: WikiExtractResponse = response.json().await.ok()?;
    let extract = data.query.pages.values().find_map(|page| page.extract.clone())?;
    if extract.trim().is_empty() {
        return None;
    }
    let url = format!(
        "https://{lang}.wikipedia.org/wiki/{}",
        urlencoding::encode(&title.replace(' ', "_"))
    );
    Some((extract.trim().to_string(), format!("Wikipedia · {lang}"), url))
}

async fn github(client: &Client, name: &str) -> Option<(String, String, String)> {
    let url = format!(
        "https://api.github.com/search/repositories?q={}&sort=stars&per_page=1",
        urlencoding::encode(name)
    );
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let data: GithubSearchResponse = response.json().await.ok()?;
    let repo = data.items.first()?;
    let description = repo.description.as_ref()?.trim();
    if description.is_empty() {
        return None;
    }
    Some((description.to_string(), "GitHub".into(), repo.html_url.clone()))
}
