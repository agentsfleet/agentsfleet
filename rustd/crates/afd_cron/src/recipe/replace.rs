//! Setup and recipe changes serialize on the same parent and child rows.

use super::{Inherited, Recipe, RecipeSet, guard, retire};
use crate::{Result, Schedules, error::ErrorKind};
use sqlx::PgConnection;

impl Schedules {
    /// Copies schedule bases on the caller's setup transaction, preserving overlays.
    ///
    /// # Errors
    /// Refuses foreign fleets, capacity overflow and a live scheduler mutation;
    /// the caller must roll back setup writes when any refusal is returned.
    pub async fn replace_recipes_on(
        &self,
        connection: &mut PgConnection,
        set: RecipeSet<'_>,
    ) -> Result<()> {
        let rows = guard::lock(connection, &set).await?;
        for (index, recipe) in set.recipes.iter().enumerate() {
            let slot = i64::try_from(index).map_err(|_overflow| ErrorKind::Exhausted)?;
            let old = rows.iter().find(|row| row.slot == slot);
            self.write_recipe(connection, &set, recipe, slot, old)
                .await?;
        }
        retire::removed(connection, &rows, &set).await
    }
}

pub(super) fn inherited(
    old: Option<&str>,
    recipe: &Recipe,
    version: i64,
    slot: i64,
) -> Result<Inherited> {
    let Some(old) = old else {
        return Ok(Inherited {
            slot,
            base: recipe.clone(),
            recipe_revision: 1,
            setup_revision: version,
            override_revision: 0,
            overrides: None,
        });
    };
    let mut old: Inherited = serde_json::from_str(old)?;
    if old.base != *recipe {
        old.recipe_revision = old
            .recipe_revision
            .checked_add(1)
            .ok_or(ErrorKind::Exhausted)?;
    }
    old.base = recipe.clone();
    old.setup_revision = version;
    Ok(old)
}
