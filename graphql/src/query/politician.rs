use crate::{context::ApiContext, relay, types::PoliticianResult};
use async_graphql::{Context, Object, Result, ID};
use db::{
    loaders::politician::{PoliticianId, PoliticianSlug},
    Politician, PoliticianFilter,
};

#[derive(Default, Debug)]
pub struct PoliticianQuery;

#[allow(clippy::too_many_arguments)]
#[Object]
impl PoliticianQuery {
    async fn politician_by_id(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> Result<Option<PoliticianResult>> {
        let politician = ctx
            .data::<ApiContext>()?
            .loaders
            .politician_loader
            .load_one(PoliticianId(uuid::Uuid::parse_str(&id)?))
            .await?;

        Ok(politician.map(PoliticianResult::from))
    }

    async fn politician_by_slug(
        &self,
        ctx: &Context<'_>,
        slug: String,
    ) -> Result<Option<PoliticianResult>> {
        let politician = ctx
            .data::<ApiContext>()?
            .loaders
            .politician_loader
            .load_one(PoliticianSlug(slug.clone()))
            .await?;

        Ok(politician.map(PoliticianResult::from))
    }

    #[allow(clippy::needless_collect)]
    async fn politicians(
        &self,
        ctx: &Context<'_>,
        filter: Option<PoliticianFilter>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> relay::ConnectionResult<PoliticianResult> {
        let db_pool = ctx.data::<ApiContext>()?.pool.clone();
        let records = Politician::filter(&db_pool, &filter.unwrap_or_default()).await?;
        let results: Vec<PoliticianResult> =
            records.into_iter().map(PoliticianResult::from).collect();

        relay::query(
            results.into_iter(),
            relay::Params::new(after, before, first, last),
            10,
        )
        .await
    }
}
