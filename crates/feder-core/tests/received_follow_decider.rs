use feder_core::{
    Activity, CoreError, DecisionContext, Effect, FederConfig, FederCore, FollowPolicyDecision,
    FollowRelationship, PlannedDelivery, ReceivedFollowState, RemoteActorState, StateChange, vocab,
};

fn iri(value: &str) -> vocab::Iri {
    value.parse().expect("valid test IRI")
}

fn actor(id: &str) -> vocab::Actor {
    vocab::Actor::person(
        iri(id),
        iri(&format!("{id}/inbox")),
        iri(&format!("{id}/outbox")),
    )
}

fn core() -> FederCore {
    FederCore::new(FederConfig::new(actor("https://example.com/users/alice")))
}

fn follow() -> vocab::Follow {
    vocab::Follow::new(
        iri("https://remote.example/activities/follow/1"),
        vocab::Reference::id(iri("https://remote.example/users/bob")),
        vocab::Reference::id(iri("https://example.com/users/alice")),
    )
}

fn received_follow_state(
    relationship: FollowRelationship,
    inbox: Option<&str>,
    shared_inbox: Option<&str>,
) -> ReceivedFollowState {
    ReceivedFollowState {
        already_processed: false,
        relationship,
        remote_actor: Some(RemoteActorState {
            actor_id: iri("https://remote.example/users/bob"),
            inbox: inbox.map(iri),
            shared_inbox: shared_inbox.map(iri),
        }),
    }
}

fn decision_context() -> DecisionContext {
    DecisionContext {
        local_actor: iri("https://example.com/users/alice"),
        accept_id: iri("https://example.com/activities/accept/1"),
    }
}

#[test]
fn accepts_new_follower() {
    let follow = follow();
    let decision = core()
        .decide_received_follow(
            follow.clone(),
            received_follow_state(
                FollowRelationship::NotFollowing,
                Some("https://remote.example/users/bob/inbox"),
                None,
            ),
            FollowPolicyDecision::Accept,
            decision_context(),
        )
        .expect("follow decision succeeds");

    assert_eq!(
        decision.state_changes[0],
        StateChange::RecordProcessedActivity {
            activity_id: iri("https://remote.example/activities/follow/1")
        }
    );
    assert_eq!(
        decision.state_changes[1],
        StateChange::AddFollower {
            local_actor: iri("https://example.com/users/alice"),
            remote_actor: iri("https://remote.example/users/bob"),
            inbox: Some(iri("https://remote.example/users/bob/inbox")),
            shared_inbox: None,
        }
    );

    let accept = match &decision.state_changes[2] {
        StateChange::StoreActivity {
            activity: Activity::Accept(accept),
        } => accept,
        _ => panic!("expected stored Accept activity"),
    };
    assert_eq!(accept.id, iri("https://example.com/activities/accept/1"));
    assert_eq!(
        accept.actor,
        vocab::Reference::id(iri("https://example.com/users/alice"))
    );
    assert_eq!(accept.object, vocab::Reference::object(follow));

    assert_eq!(
        decision.effects,
        [Effect::PlanDelivery(PlannedDelivery {
            activity: Activity::Accept(accept.clone()),
            inbox: iri("https://remote.example/users/bob/inbox"),
        })]
    );
}

#[test]
fn uses_shared_inbox_for_delivery() {
    let decision = core()
        .decide_received_follow(
            follow(),
            received_follow_state(
                FollowRelationship::NotFollowing,
                Some("https://remote.example/users/bob/inbox"),
                Some("https://remote.example/inbox"),
            ),
            FollowPolicyDecision::Accept,
            decision_context(),
        )
        .expect("follow decision succeeds");

    match &decision.effects[0] {
        Effect::PlanDelivery(delivery) => {
            assert_eq!(delivery.inbox, iri("https://remote.example/inbox"));
        }
        _ => panic!("expected planned delivery"),
    }
}

#[test]
fn already_processed_activity_is_idempotent() {
    let mut state = received_follow_state(
        FollowRelationship::NotFollowing,
        Some("https://remote.example/users/bob/inbox"),
        None,
    );
    state.already_processed = true;

    let decision = core()
        .decide_received_follow(
            follow(),
            state,
            FollowPolicyDecision::Accept,
            decision_context(),
        )
        .expect("follow decision succeeds");

    assert!(decision.is_empty());
}

#[test]
fn existing_follower_is_not_added_again() {
    let decision = core()
        .decide_received_follow(
            follow(),
            received_follow_state(
                FollowRelationship::Following,
                Some("https://remote.example/users/bob/inbox"),
                None,
            ),
            FollowPolicyDecision::Accept,
            decision_context(),
        )
        .expect("follow decision succeeds");

    assert_eq!(decision.state_changes.len(), 2);
    assert!(matches!(
        decision.state_changes[0],
        StateChange::RecordProcessedActivity { .. }
    ));
    assert!(matches!(
        decision.state_changes[1],
        StateChange::StoreActivity { .. }
    ));
    assert_eq!(decision.effects.len(), 1);
}

#[test]
fn missing_inbox_returns_error() {
    let err = core()
        .decide_received_follow(
            follow(),
            received_follow_state(FollowRelationship::NotFollowing, None, None),
            FollowPolicyDecision::Accept,
            decision_context(),
        )
        .expect_err("missing inbox should fail");

    assert_eq!(err, CoreError::MissingInbox);
}

#[test]
fn non_accept_policy_has_no_protocol_side_effects() {
    for policy in [
        FollowPolicyDecision::Reject,
        FollowPolicyDecision::RequireManualApproval,
    ] {
        let decision = core()
            .decide_received_follow(
                follow(),
                received_follow_state(
                    FollowRelationship::NotFollowing,
                    Some("https://remote.example/users/bob/inbox"),
                    None,
                ),
                policy,
                decision_context(),
            )
            .expect("follow decision succeeds");

        assert!(decision.is_empty());
    }
}
