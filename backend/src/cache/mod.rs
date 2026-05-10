use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct CachedUser {
    pub pseudo: String,
    pub email: String,
}

pub struct UserCache {
    uuid_to_user: RwLock<HashMap<Uuid, CachedUser>>,
    pseudo_to_uuid: RwLock<HashMap<String, Uuid>>,
}

#[allow(dead_code)]
impl UserCache {
    pub fn new() -> Self {
        Self {
            uuid_to_user: RwLock::new(HashMap::new()),
            pseudo_to_uuid: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get_by_pseudo(&self, pseudo: &str) -> Option<CachedUser> {
        let reverse = self.pseudo_to_uuid.read().await;
        let uuid = reverse.get(pseudo)?;
        let forward = self.uuid_to_user.read().await;
        forward.get(uuid).cloned()
    }

    pub async fn get_uuid_by_pseudo(&self, pseudo: &str) -> Option<Uuid> {
        let reverse = self.pseudo_to_uuid.read().await;
        reverse.get(pseudo).copied()
    }

    pub async fn get_by_uuid(&self, uuid: &Uuid) -> Option<CachedUser> {
        let forward = self.uuid_to_user.read().await;
        forward.get(uuid).cloned()
    }

    pub async fn put(&self, uuid: Uuid, pseudo: String, email: String) {
        let mut forward = self.uuid_to_user.write().await;
        forward.insert(
            uuid,
            CachedUser {
                pseudo: pseudo.clone(),
                email,
            },
        );
        drop(forward);
        let mut reverse = self.pseudo_to_uuid.write().await;
        reverse.insert(pseudo, uuid);
    }

    pub async fn populate_bulk(&self, users: &[(Uuid, String, String)]) {
        let mut forward = self.uuid_to_user.write().await;
        let mut reverse = self.pseudo_to_uuid.write().await;
        for (uuid, pseudo, email) in users {
            forward.insert(
                *uuid,
                CachedUser {
                    pseudo: pseudo.clone(),
                    email: email.clone(),
                },
            );
            reverse.insert(pseudo.clone(), *uuid);
        }
    }

    pub async fn invalidate(&self, uuid: &Uuid) {
        let mut forward = self.uuid_to_user.write().await;
        if let Some(user) = forward.remove(uuid) {
            drop(forward);
            let mut reverse = self.pseudo_to_uuid.write().await;
            reverse.remove(&user.pseudo);
        }
    }
}

impl Default for UserCache {
    fn default() -> Self {
        Self::new()
    }
}
