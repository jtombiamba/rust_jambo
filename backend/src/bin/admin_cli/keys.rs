use uuid::Uuid;

use jambo_backend::auth::password::hash_password;
use jambo_backend::database::repositories::AdminKeyRepository;

use super::authenticate;

pub(crate) async fn keys_add(repo: &AdminKeyRepository, label: &str) -> Result<(), String> {
    let active = repo.count_active().await.map_err(|e| format!("db: {e}"))?;
    if active > 0 {
        // Protect the table once initialised.
        let _ = authenticate(repo).await?;
    }
    let keypass = generate_keypass();
    let hash = hash_password(&keypass).map_err(|e| e.to_string())?;
    repo.create(label, &hash)
        .await
        .map_err(|e| format!("db: {e}"))?;
    println!("Created admin key '{label}'. Keypass (shown once, store safely):");
    println!("{keypass}");
    Ok(())
}

pub(crate) async fn keys_list(repo: &AdminKeyRepository) -> Result<(), String> {
    let _ = authenticate(repo).await?;
    let keys = repo.list_active().await.map_err(|e| format!("db: {e}"))?;
    for key in keys {
        println!("{}\t{}", key.label, key.created_at);
    }
    Ok(())
}

pub(crate) async fn keys_revoke(repo: &AdminKeyRepository, label: &str) -> Result<(), String> {
    let _ = authenticate(repo).await?;
    let rows = repo.revoke(label).await.map_err(|e| format!("db: {e}"))?;
    if rows == 0 {
        return Err(format!("no active key found with label '{label}'"));
    }
    println!("revoked '{label}'");
    Ok(())
}

fn generate_keypass() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
