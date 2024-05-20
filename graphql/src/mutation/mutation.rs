use super::{
    argument::ArgumentMutation, auth::AuthMutation, ballot_measure::BallotMeasureMutation,
    bill::BillMutation, candidate_guide::CandidateGuideMutation, election::ElectionMutation,
    embed::EmbedMutation, issue_tag::IssueTagMutation, office::OfficeMutation,
    organization::OrganizationMutation, politician::PoliticianMutation, poll::PollMutation,
    question::QuestionMutation, race::RaceMutation, user::UserMutation,
    voting_guide::VotingGuideMutation,
};
use async_graphql::MergedObject;
#[derive(MergedObject, Default)]
pub struct Mutation(
    ArgumentMutation,
    PoliticianMutation,
    OrganizationMutation,
    BillMutation,
    BallotMeasureMutation,
    CandidateGuideMutation,
    ElectionMutation,
    EmbedMutation,
    IssueTagMutation,
    AuthMutation,
    OfficeMutation,
    RaceMutation,
    VotingGuideMutation,
    UserMutation,
    PollMutation,
    QuestionMutation,
);
