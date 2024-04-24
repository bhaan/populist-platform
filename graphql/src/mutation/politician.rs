use crate::{
    context::ApiContext,
    guard::StaffOnly,
    is_admin,
    types::{Error, PoliticianResult},
    upload_to_s3, File,
};
use async_graphql::{Error as GraphQLError, *};
use db::{
    CreateOrConnectIssueTagInput, CreateOrConnectOrganizationInput, CreateOrConnectPoliticianInput,
    IssueTag, Organization, OrganizationIdentifier, Politician, PoliticianIdentifier,
    UpsertPoliticianInput,
};
use sqlx::{Pool, Postgres};
use std::io::Read;

use std::{fmt::format, str::FromStr};
#[derive(Default)]
pub struct PoliticianMutation;

#[derive(SimpleObject)]
#[graphql(visible = "is_admin")]
struct DeletePoliticianResult {
    id: String,
}

// Create or connect issue tags with relation to new or updated politician
async fn handle_nested_issue_tags(
    db_pool: &Pool<Postgres>,
    politician_id: uuid::Uuid,
    issue_tags_input: CreateOrConnectIssueTagInput,
) -> Result<(), Error> {
    if issue_tags_input.create.is_some() {
        for input in issue_tags_input.create.unwrap() {
            let new_issue_tag = IssueTag::upsert(db_pool, &input).await?;
            Politician::connect_issue_tag(db_pool, politician_id, new_issue_tag.id).await?;
        }
    }
    if issue_tags_input.connect.is_some() {
        for issue_tag_id in issue_tags_input.connect.unwrap() {
            // figure out how to accept slugs and IDs here, that'd be great
            Politician::connect_issue_tag(
                db_pool,
                politician_id,
                uuid::Uuid::parse_str(&issue_tag_id)?,
            )
            .await?;
        }
    }
    Ok(())
}

async fn handle_nested_organization_endorsements(
    db_pool: &Pool<Postgres>,
    politician_id: uuid::Uuid,
    organizations_input: CreateOrConnectOrganizationInput,
) -> Result<(), Error> {
    if organizations_input.create.is_some() {
        for input in organizations_input.create.unwrap() {
            let new_organization = Organization::upsert(db_pool, &input).await?;
            Politician::connect_organization(
                db_pool,
                politician_id,
                OrganizationIdentifier::Uuid(new_organization.id),
            )
            .await?;
        }
    }
    if organizations_input.connect.is_some() {
        for organization_identifier in organizations_input.connect.unwrap() {
            match uuid::Uuid::from_str(organization_identifier.as_str()) {
                Ok(org_id) => {
                    Politician::connect_organization(
                        db_pool,
                        politician_id,
                        OrganizationIdentifier::Uuid(org_id),
                    )
                    .await?
                }
                _ => {
                    Politician::connect_organization(
                        db_pool,
                        politician_id,
                        OrganizationIdentifier::Slug(organization_identifier),
                    )
                    .await?
                }
            };
        }
    }

    Ok(())
}

async fn handle_nested_politician_endorsements(
    db_pool: &Pool<Postgres>,
    politician_id: uuid::Uuid,
    politicians_input: CreateOrConnectPoliticianInput,
) -> Result<(), Error> {
    if politicians_input.create.is_some() {
        for input in politicians_input.create.unwrap() {
            let new_politician = Politician::upsert(db_pool, &input).await?;
            Politician::connect_politician(
                db_pool,
                politician_id,
                PoliticianIdentifier::Uuid(new_politician.id),
            )
            .await?;
        }
    }
    if politicians_input.connect.is_some() {
        for politician_identifier in politicians_input.connect.unwrap() {
            match uuid::Uuid::from_str(politician_identifier.as_str()) {
                Ok(pol_endorsement_id) => {
                    Politician::connect_politician(
                        db_pool,
                        politician_id,
                        PoliticianIdentifier::Uuid(pol_endorsement_id),
                    )
                    .await?
                }
                _ => {
                    Politician::connect_politician(
                        db_pool,
                        politician_id,
                        PoliticianIdentifier::Slug(politician_identifier),
                    )
                    .await?
                }
            };
        }
    }

    Ok(())
}

#[Object]
impl PoliticianMutation {
    #[graphql(guard = "StaffOnly", visible = "is_admin")]
    async fn upsert_politician(
        &self,
        ctx: &Context<'_>,
        input: UpsertPoliticianInput,
    ) -> Result<PoliticianResult> {
        let db_pool = ctx.data::<ApiContext>()?.pool.clone();
        let new_record = Politician::upsert(&db_pool, &input).await?;
        // be sure to handle None inputs from GraphQL
        if input.issue_tags.is_some() {
            handle_nested_issue_tags(&db_pool, new_record.id, input.issue_tags.unwrap()).await?;
        }

        if input.organization_endorsements.is_some() {
            handle_nested_organization_endorsements(
                &db_pool,
                new_record.id,
                input.organization_endorsements.unwrap(),
            )
            .await?;
        }

        if input.politician_endorsements.is_some() {
            handle_nested_politician_endorsements(
                &db_pool,
                new_record.id,
                input.politician_endorsements.unwrap(),
            )
            .await?;
        }

        Ok(PoliticianResult::from(new_record))
    }

    #[graphql(guard = "StaffOnly", visible = "is_admin")]
    async fn delete_politician(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> Result<DeletePoliticianResult> {
        let db_pool = ctx.data::<ApiContext>()?.pool.clone();
        Politician::delete(&db_pool, uuid::Uuid::parse_str(&id)?).await?;
        Ok(DeletePoliticianResult { id })
    }

    #[graphql(guard = "StaffOnly", visible = "is_admin")]
    async fn upload_politician_picture(
        &self,
        ctx: &Context<'_>,
        slug: String,
        file: Upload,
    ) -> Result<String> {
        let db_pool = ctx.data::<ApiContext>()?.pool.clone();

        let upload = file.value(ctx).unwrap();
        let mut content = Vec::new();
        let filename = format!("{}-400", slug);
        let mimetype = upload.content_type.clone();

        upload.into_read().read_to_end(&mut content).unwrap();
        let file_info = File {
            id: ID::from(uuid::Uuid::new_v4()),
            filename,
            content,
            mimetype,
        };
        let url = upload_to_s3(file_info, "web-assets/politician-thumbnails".to_string()).await?;
        // Append last modified date because s3 path will remain the same and we want browser to cache, but refresh the image
        let url = format!("{}{}{}", url, "?lastmod=", chrono::Utc::now().timestamp());

        let result = sqlx::query!(
            r#"
            UPDATE politician SET assets = jsonb_set(jsonb_set(assets, '{thumbnailImage160}', $1::jsonb, true), '{thumbnailImage400}', $1::jsonb, true)
            WHERE slug = $2
            RETURNING assets
        "#,
            serde_json::json!(url), // Convert url to JSON format
            slug
        )
        .fetch_one(&db_pool)
        .await;

        match result {
            Ok(_) => Ok(url),
            Err(err) => {
                tracing::error!("{}", err.to_string());
                Err(GraphQLError::from(err))
            }
        }
    }
}
