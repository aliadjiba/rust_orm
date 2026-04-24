use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use surrealdb::sql::Thing;

use crate::{model::{Model, Pivot, query::Query}, repository::{ErrorIO, Repo}};




/* ===========================
   BELONGS TO MANY
=========================== */

pub struct BelongsToMany<'a, P>
where
    P: Pivot,
{
    repo: &'a Repo,
    parent_id: Thing,
    is_left: bool, // 🔥 this decides direction
    _pivot: PhantomData<P>,
}


impl<'a, P> BelongsToMany<'a, P>
where
    P: Pivot + Serialize  + 'static,
{
    pub fn new(repo: &'a Repo, parent_id: Thing, is_left: bool) -> Self {
        Self {
            repo,
            parent_id,
            is_left,
            _pivot: PhantomData,
        }
    }

    /// Attach single relation
pub async fn attach(&self, related_id: Thing) -> Result<(), ErrorIO> {
    let pivot = if self.is_left {
        P::new(self.parent_id.clone(), related_id)
    } else {
        P::new(related_id, self.parent_id.clone())
    };

    Query::<P>::new(self.repo)
        .insert()
        .values(pivot)
        .await?;

    Ok(())
}

pub async fn attach_with<F>(&self, related_id: Thing, builder: F) -> Result<(), ErrorIO>
where
    F: FnOnce(P) -> P,
{
    let pivot = if self.is_left {
        P::new(self.parent_id.clone(), related_id)
    } else {
        P::new(related_id, self.parent_id.clone())
    };

    let pivot = builder(pivot);

    Query::<P>::new(self.repo)
        .insert()
        .values(pivot)
        .await?;

    Ok(())
}


    /// Detach relation
pub async fn detach(&self, related_id: Thing) -> Result<(), ErrorIO> {
    let mut query = Query::<P>::new(self.repo);

    if self.is_left {
        query = query
            .where_eq(P::left_key(), self.parent_id.clone())
            .where_eq(P::right_key(), related_id);
    } else {
        query = query
            .where_eq(P::right_key(), self.parent_id.clone())
            .where_eq(P::left_key(), related_id);
    }

    let q =query.first().await?;
    match q  {
        Some(p)=>{P::delete(self.repo).by_id(p.id());},
        None=>{}
    }
    Ok(())
}


    /// Load related IDs
async fn existing_related_ids(&self) -> Result<Vec<Thing>, ErrorIO> {
    let pivots = if self.is_left {
        Query::<P>::new(self.repo)
            .where_eq(P::left_key(), self.parent_id.clone())
            .all()
            .await?
    } else {
        Query::<P>::new(self.repo)
            .where_eq(P::right_key(), self.parent_id.clone())
            .all()
            .await?
    };

    Ok(pivots
        .into_iter()
        .map(|p| {
            if self.is_left {
                p.right_id().clone()
            } else {
                p.left_id().clone()
            }
        })
        .collect())
}


    /// sync() — Laravel style
    pub async fn sync(&self, ids: Vec<Thing>) -> Result<(), ErrorIO> {
        let existing = self.existing_related_ids().await?;

        let to_attach: Vec<_> = ids
            .iter()
            .filter(|id| !existing.contains(id))
            .cloned()
            .collect();

        let to_detach: Vec<_> = existing
            .into_iter()
            .filter(|id| !ids.contains(id))
            .collect();

        for id in to_attach {
            self.attach(id).await?;
        }

        for id in to_detach {
            self.detach(id).await?;
        }

        Ok(())
    }

    /// sync_without_detach()
    pub async fn sync_without_detach(&self, ids: Vec<Thing>) -> Result<(), ErrorIO> {
        let existing = self.existing_related_ids().await?;

        let to_attach: Vec<_> = ids
            .into_iter()
            .filter(|id| !existing.contains(id))
            .collect();

        for id in to_attach {
            self.attach(id).await?;
        }

        Ok(())
    }
    pub async fn load<R>(&self) -> Result<Vec<R>, ErrorIO>
where
    R: Model + Clone,
{
    let pivots = if self.is_left {
        Query::<P>::new(self.repo)
            .where_eq(P::left_key(), self.parent_id.clone())
            .all()
            .await?
    } else {
        Query::<P>::new(self.repo)
            .where_eq(P::right_key(), self.parent_id.clone())
            .all()
            .await?
    };

    if pivots.is_empty() {
        return Ok(vec![]);
    }

    let related_ids: Vec<Thing> = pivots
        .into_iter()
        .map(|p| {
            if self.is_left {
                p.right_id().clone()
            } else {
                p.left_id().clone()
            }
        })
        .collect();

    let related = Query::<R>::new(self.repo)
        .where_in("id", related_ids)
        .all()
        .await?;

    Ok(related)
}

}



#[derive(Serialize,Deserialize,Debug)]
pub struct BelongsToManyType{
    id: Thing,
    parent: Thing,
    child: Thing,
}
