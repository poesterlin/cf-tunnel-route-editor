use serde_derive::Deserialize;
use serde_derive::Serialize;
use serde_json::json;
use serde_json::Value;
use std::env;

enum ControlFlow {
    Continue,
    Break,
}

fn is_none_or_empty(val: &Option<String>) -> bool {
    if let Some(value) = val {
        return value.to_string().is_empty();
    }

    return true;
}

fn is_none_or_empty_map(val: &Option<OriginRequest>) -> bool {
    if let Some(value) = val {
        return !value.is_populated();
    }

    return true;
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CFConfig {
    pub success: bool,
    pub errors: Vec<Value>,
    pub messages: Vec<Value>,
    pub result: Result,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Result {
    #[serde(rename = "tunnel_id")]
    pub tunnel_id: String,
    pub config: Config,
    pub version: i64,
    #[serde(skip_serializing)]
    pub source: String,
    #[serde(skip_serializing)]
    #[serde(rename = "created_at")]
    pub created_at: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub ingress: Vec<Ingress>,
    #[serde(rename = "warp-routing")]
    pub warp_routing: WarpRouting,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ingress {
    #[serde(skip_serializing_if = "is_none_or_empty")]
    pub id: Option<String>,
    pub service: String,
    #[serde(skip_serializing_if = "is_none_or_empty")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "is_none_or_empty_map")]
    pub origin_request: Option<OriginRequest>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "http2Origin")]
    pub http2origin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "noTLSVerify")]
    pub no_tlsverify: Option<bool>,
    #[serde(skip_serializing_if = "is_none_or_empty")]
    pub http_host_header: Option<String>,
}

impl OriginRequest {
    fn is_populated(&self) -> bool {
        self.http2origin.unwrap_or(false)
            || self.no_tlsverify.unwrap_or(false)
            || !self
                .http_host_header
                .clone()
                .unwrap_or("".to_string())
                .is_empty()
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarpRouting {
    pub enabled: bool,
}

pub async fn post_config_to_cloudflare(host_name: String) {
    let cf_token = env::var("CF_TOKEN").expect("CF_TOKEN in not defined");
    let cf_account_id = env::var("CF_ACCOUNT_ID").expect("CF_ACCOUNT_ID in not defined");
    let cf_tunnel_id = env::var("CF_TUNNEL_ID").expect("CF_TUNNEL_ID in not defined");
    let cf_zone_id = env::var("CF_ZONE_ID").expect("CF_ZONE_ID in not defined");
    validate_cf_token(cf_token.clone()).await;

    // get tunnel config
    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/cfd_tunnel/{}/configurations",
            cf_account_id, cf_tunnel_id
        ))
        .header("Authorization", format!("Bearer {}", cf_token))
        .send()
        .await;

    if let Err(_) = res {
        println!("could not request tunnel config");
        return;
    }

    let res = res.unwrap();
    if !res.status().is_success() {
        println!("could not get tunnel config");
        return;
    }

    let text = match res.text().await {
        Err(_) => {
            println!("could not get tunnel config");
            return;
        }
        Ok(text) => text,
    };

    let cf_response: CFConfig = match serde_json::from_str(&text) {
        Err(error) => {
            println!("could not parse tunnel config: {}", error);
            return;
        }
        Ok(cf_config) => cf_config,
    };

    let cf_config_orig = cf_response.result.clone();
    let mut cf_config = cf_config_orig.clone();

    // check if ingress exists
    let exists = cf_config
        .config
        .ingress
        .iter()
        .any(|ingress| ingress.hostname == Some(host_name.clone()));

    if exists {
        println!("ingress already exists");
        return;
    }

    // find id for new ingress
    let highest_id = cf_config.config.ingress.iter().fold(0, |acc, ingress| {
        let id = ingress
            .id
            .clone()
            .unwrap_or("0".to_string())
            .parse::<u8>()
            .unwrap_or(0);
        if id > acc {
            id
        } else {
            acc
        }
    });

    // create new ingress
    let new_ingress = Ingress {
        id: Some((highest_id + 1).to_string()),
        service: "https://traefik".to_string(),
        hostname: Some(host_name.clone()),
        origin_request: Some(OriginRequest {
            http2origin: Some(true),
            no_tlsverify: Some(true),
            http_host_header: None,
        }),
    };

    // catch all ingress always needs to be last
    let last = cf_config.config.ingress.pop();

    cf_config.config.ingress.push(new_ingress);

    if let Some(catch_all_ingress) = last {
        cf_config.config.ingress.push(catch_all_ingress);
    }

    if let ControlFlow::Break =
        apply_cf_config(&cf_config, &cf_account_id, &cf_tunnel_id, &cf_token).await
    {
        println!("could not apply new config");
        return;
    }

    println!("new ingress created");

    // create dns record
    let res = client
        .post(format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records",
            cf_zone_id
        ))
        .header("Authorization", format!("Bearer {}", cf_token))
        .header("Content-Type", "application/json")
        .body(
            serde_json::to_string(&json!({
                "type": "CNAME",
                "proxied": true,
                "name": host_name,
                "content": format!("{}.cfargotunnel.com", cf_tunnel_id)
            }))
            .unwrap(),
        )
        .send()
        .await;

    if let Err(_) = res {
        println!("could not create dns record, reverting changes");
        if let ControlFlow::Break =
            apply_cf_config(&cf_config_orig, &cf_account_id, &cf_tunnel_id, &cf_token).await
        {
            panic!("could not revert changes after failed dns record creation");
        }

        println!("changes reverted");
        return;
    }

    let res = res.unwrap();
    if !res.status().is_success() {
        println!("could not create dns record, reverting changes");
        if let ControlFlow::Break =
            apply_cf_config(&cf_config_orig, &cf_account_id, &cf_tunnel_id, &cf_token).await
        {
            panic!("could not revert changes after failed dns record creation");
        }
        println!("changes reverted");
        return;
    }

    let text = res
        .text()
        .await
        .unwrap_or_else(|_| "could not get response text".to_string());
    println!("{}", text);

    println!("dns record created");
    println!("tunnel is now available at: {}", host_name);
}

async fn apply_cf_config(
    cf_config: &Result,
    cf_account_id: &String,
    cf_tunnel_id: &String,
    cf_token: &String,
) -> ControlFlow {
    let updated_config = serde_json::to_string_pretty(&cf_config).unwrap();
    let client = reqwest::Client::new();
    let res = client
        .put(format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/cfd_tunnel/{}/configurations",
            cf_account_id, cf_tunnel_id
        ))
        .header("Authorization", format!("Bearer {}", cf_token))
        .header("Content-Type", "application/json")
        .body(updated_config)
        .send()
        .await;

    if let Err(_) = res {
        println!("could not update tunnel config, could not send request");
        return ControlFlow::Break;
    }

    let res = res.unwrap();
    if !res.status().is_success() {
        println!("could not update tunnel config");
        let text = res
            .text()
            .await
            .unwrap_or_else(|_| "could not get response text".to_string());
        println!("{}", text);
        return ControlFlow::Break;
    }

    return ControlFlow::Continue;
}

async fn validate_cf_token(cf_token: String) {
    let client = reqwest::Client::new();
    let res = client
        .get("https://api.cloudflare.com/client/v4/user/tokens/verify")
        .header("Authorization", format!("Bearer {}", cf_token))
        .header("Content-Type", "application/json")
        .send()
        .await;

    match res {
        Ok(res) => {
            if res.status().is_success() {
                println!("CF Token is valid");
            } else {
                println!("CF Token is invalid");
            }
        }
        Err(_) => println!("CF Token is invalid"),
    }
}
