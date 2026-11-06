//! Who may put a clip on a public link: a share link, or a collection's link that serves it.
use crate::auth::User;
use reel_store::models::{Clip, Collection, CollectionLink};

/// A link's maker, as far as what the link may serve goes.
#[derive(Debug, Clone)]
pub struct Sharer {
    id: String,
    admin: bool,
}

impl Sharer {
    pub fn of(user: &User) -> Sharer {
        Sharer {
            id: user.id.clone(),
            admin: user.admin,
        }
    }

    /// An admin any clip; anyone else a clip of their own that holds no other user's footage.
    pub fn may_share(&self, clip: &Clip) -> bool {
        self.admin || (clip.owner_id.as_deref() == Some(self.id.as_str()) && !clip.others_footage)
    }
}

/// Who a collection's link serves for: its maker; for a link made before reel kept the maker,
/// the collection's owner; for a game's own collection, whose links admins make, an admin.
pub async fn sharer_of_link(
    store: &reel_store::Store,
    link: &CollectionLink,
    collection: &Collection,
) -> anyhow::Result<Sharer> {
    let Some(id) = link.made_by.clone().or_else(|| collection.owner_id.clone()) else {
        return Ok(Sharer {
            id: String::new(),
            admin: true,
        });
    };
    let admin = store.user(&id).await?.is_some_and(|u| u.is_admin());
    Ok(Sharer { id, admin })
}

/// Why a clip gets no link from someone who may change it otherwise.
pub const OTHERS_FOOTAGE: &str =
    "this clip holds footage of another user's clips: only an admin makes a link for it";
