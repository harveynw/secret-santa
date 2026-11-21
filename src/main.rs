use secret_santa::{App, Config, router, store::Store};
use std::{env, sync::Arc};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let site_access_token =
        env::var("SITE_ACCESS_TOKEN").map_err(|_| "SITE_ACCESS_TOKEN is required")?;
    if site_access_token.len() < 32
        || !site_access_token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("SITE_ACCESS_TOKEN must contain at least 32 random URL-safe characters (letters, digits, - or _)".into());
    }
    let base_url =
        env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "https://santa.example.com".into());
    let url = reqwest::Url::parse(&base_url)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err("PUBLIC_BASE_URL must be an HTTPS origin".into());
    }
    let nonempty = |name| env::var(name).ok().filter(|v| !v.trim().is_empty());
    let admin_access_token = nonempty("ADMIN_ACCESS_TOKEN");
    if admin_access_token
        .as_ref()
        .is_some_and(|token| token.len() < 32 || token == &site_access_token)
    {
        return Err("ADMIN_ACCESS_TOKEN must be at least 32 characters and different from SITE_ACCESS_TOKEN".into());
    }
    let group_name = nonempty("GROUP_NAME")
        .map(|name| name.trim().to_owned())
        .unwrap_or_else(|| "Secret Santa".into());
    if group_name.chars().count() > 100 || group_name.chars().any(char::is_control) {
        return Err("GROUP_NAME must be at most 100 characters without control characters".into());
    }
    let config = Config {
        group_name,
        admin_access_token,
        site_access_token,
        base_url: base_url.trim_end_matches('/').into(),
        resend_key: nonempty("RESEND_API_KEY"),
        email_from: nonempty("EMAIL_FROM"),
    };
    let store = Store::open(secret_santa::config::DATABASE_PATH)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    let local_origin = format!("http://{}", listener.local_addr()?);
    let mut signup_link = reqwest::Url::parse(&format!("{}/signup.html", config.base_url))?;
    signup_link
        .query_pairs_mut()
        .append_pair("token", &config.site_access_token);
    println!("\x1b[1;36mSignup:\x1b[0m {signup_link}");
    if let Some(token) = &config.admin_access_token {
        let mut admin_link = reqwest::Url::parse(&format!("{}/admin.html", config.base_url))?;
        admin_link.query_pairs_mut().append_pair("token", token);
        println!("\x1b[1;36mAdmin:\x1b[0m {admin_link}");
    } else {
        println!("\x1b[1;36mAdmin:\x1b[0m disabled (set ADMIN_ACCESS_TOKEN to enable)");
    }
    let invitation = format!(
        "🎄 {} — Secret Santa\n\nJoin our Christmas gift exchange! Sign up and confirm your email to take part:\n{}",
        config.group_name, signup_link
    );
    let mut whatsapp_link = reqwest::Url::parse("https://wa.me/")?;
    whatsapp_link
        .query_pairs_mut()
        .append_pair("text", &invitation);
    println!("\x1b[1;36mCopyable WhatsApp link:\x1b[0m {whatsapp_link}");
    println!("\nSecret Santa listening on {local_origin}");
    let app = Arc::new(App::new(store, config));
    let mailer = tokio::spawn(secret_santa::mailer::run(app.clone()));
    let routes = router(app).fallback_service(tower_http::services::ServeDir::new(
        secret_santa::config::STATIC_DIRECTORY,
    ));
    axum::serve(listener, routes)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    mailer.abort();
    Ok(())
}
