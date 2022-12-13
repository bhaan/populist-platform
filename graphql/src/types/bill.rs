use crate::{context::ApiContext, types::ArgumentResult};
use async_graphql::{ComplexObject, Context, Result, SimpleObject, ID};
use auth::Claims;
use db::{
    models::{
        bill::Bill,
        enums::{ArgumentPosition, BillStatus, BillType, PoliticalScope, State},
    },
    Chamber, PublicVotes,
};
use jsonwebtoken::TokenData;
use legiscan::Bill as LegiscanBill;
use sqlx::{types::Json, Row};
use std::str::FromStr;
use uuid::Uuid;

use super::{IssueTagResult, PoliticianResult};
#[derive(SimpleObject)]
#[graphql(complex)]
pub struct BillResult {
    id: ID,
    slug: String,
    title: String,
    bill_number: String,
    status: BillStatus,
    description: Option<String>,
    official_summary: Option<String>,
    populist_summary: Option<String>,
    full_text_url: Option<String>,
    votesmart_bill_id: Option<i32>,
    legiscan_bill_id: Option<i32>,
    legiscan_committee_name: Option<String>,
    history: serde_json::Value,
    state: Option<State>,
    chamber: Option<Chamber>,
    bill_type: BillType,
    political_scope: PoliticalScope,
}

#[derive(SimpleObject)]
pub struct SessionResult {
    name: String,
    description: String,
    start_date: Option<chrono::NaiveDate>,
    end_date: Option<chrono::NaiveDate>,
    state: Option<State>,
    congress_name: String,
}

#[ComplexObject]
impl BillResult {
    async fn arguments(&self, ctx: &Context<'_>) -> Result<Vec<ArgumentResult>> {
        let db_pool = ctx.data::<ApiContext>()?.pool.clone();
        let records = Bill::arguments(&db_pool, uuid::Uuid::parse_str(&self.id).unwrap()).await?;
        let results = records.into_iter().map(ArgumentResult::from).collect();
        Ok(results)
    }

    async fn legiscan_data(&self, ctx: &Context<'_>) -> Result<LegiscanBill> {
        let db_pool = ctx.data::<ApiContext>()?.pool.clone();

        let record = sqlx::query(
            r#"
                SELECT legiscan_data FROM bill
                WHERE id=$1
            "#,
        )
        .bind(Uuid::parse_str(&self.id).unwrap())
        .fetch_one(&db_pool)
        .await?;

        let legiscan_data: Json<LegiscanBill> = record.get(0);

        Ok(legiscan_data.0)
    }

    async fn issue_tags(&self, ctx: &Context<'_>) -> Result<Vec<IssueTagResult>> {
        let db_pool = ctx.data::<ApiContext>()?.pool.clone();
        let records = Bill::issue_tags(&db_pool, uuid::Uuid::parse_str(&self.id).unwrap()).await?;
        let results = records.into_iter().map(IssueTagResult::from).collect();
        Ok(results)
    }

    async fn sponsors(&self, ctx: &Context<'_>) -> Result<Vec<PoliticianResult>> {
        let db_pool = ctx.data::<ApiContext>()?.pool.clone();
        let records = Bill::sponsors(&db_pool, uuid::Uuid::parse_str(&self.id).unwrap()).await?;
        let results = records.into_iter().map(PoliticianResult::from).collect();
        Ok(results)
    }

    async fn public_votes(&self, ctx: &Context<'_>) -> Result<PublicVotes> {
        let db_pool = ctx.data::<ApiContext>()?.pool.clone();
        let results = sqlx::query_as!(
            PublicVotes,
            r#"
                SELECT SUM(CASE WHEN position = 'support' THEN 1 ELSE 0 END) as support,
                       SUM(CASE WHEN position = 'neutral' THEN 1 ELSE 0 END) as neutral,
                       SUM(CASE WHEN position = 'oppose' THEN 1 ELSE 0 END) as oppose
                FROM bill_public_votes WHERE bill_id = $1
            "#,
            Uuid::parse_str(&self.id).unwrap(),
        )
        .fetch_one(&db_pool)
        .await?;

        Ok(results)
    }

    async fn users_vote(&self, ctx: &Context<'_>) -> Result<Option<ArgumentPosition>> {
        let db_pool = ctx.data::<ApiContext>()?.pool.clone();
        let token = ctx.data::<Option<TokenData<Claims>>>().unwrap().as_ref();

        match token {
            Some(token) => {
                let user_id = token.claims.sub;
                let results = sqlx::query!(
                    r#"
                        SELECT position AS "position: ArgumentPosition" FROM bill_public_votes WHERE bill_id = $1 AND user_id = $2
                    "#,
                    Uuid::parse_str(&self.id).unwrap(),
                    user_id,
                )
                .fetch_optional(&db_pool)
                .await?;
                let position = results.map(|r| r.position);
                Ok(position)
            }
            None => Ok(None),
        }
    }

    async fn session(&self, ctx: &Context<'_>) -> Result<SessionResult> {
        let db_pool = ctx.data::<ApiContext>()?.pool.clone();
        let result = sqlx::query_as!(SessionResult,
            r#"
                SELECT s.name, s.description, s.start_date, s.end_date, s.state AS "state: State", s.congress_name
                FROM session s
                INNER JOIN bill b ON b.session_id = s.id
                WHERE b.id = $1
            "#,
            Uuid::parse_str(&self.id).unwrap(),
        )
        .fetch_one(&db_pool)
        .await?;

        Ok(result)
    }
}

impl From<Bill> for BillResult {
    fn from(b: Bill) -> Self {
        Self {
            id: ID::from(b.id),
            slug: b.slug,
            title: b.title,
            bill_number: b.bill_number,
            status: b.status,
            description: b.description,
            official_summary: b.official_summary,
            populist_summary: b.populist_summary,
            full_text_url: b.full_text_url,
            votesmart_bill_id: b.votesmart_bill_id,
            legiscan_bill_id: b.legiscan_bill_id,
            legiscan_committee_name: b.legiscan_committee,
            history: b.history,
            state: b.state,
            chamber: b.chamber,
            bill_type: BillType::from_str(&b.bill_type).unwrap_or_default(),
            political_scope: b.political_scope,
        }
    }
}
