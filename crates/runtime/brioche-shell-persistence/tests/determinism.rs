//! Serialization determinism contracts.

use std::collections::BTreeMap;

use brioche_core::{AgentState, ChatMessage, Session, ToolCallDescriptor};
use brioche_shell_persistence::{
    FlattenedAgentState, SessionHeadDTO, SessionSchemaVersion, serialize_head,
};
use proptest::prelude::*;

#[test]
fn idempotence_two_serializations_bit_for_bit() {
    let mut session = Session::new("idempotent");
    session.history.push(ChatMessage::User {
        content: "hello".into(),
    });
    session.history.push(ChatMessage::Assistant {
        content: "world".into(),
        reasoning: None,
        tool_calls: Vec::new(),
    });
    match session.push_state(AgentState::Predicting { generation_id: 42 }) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let dto = SessionHeadDTO::from_session(&session);
    let blob1 = match serialize_head(&dto) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let blob2 = match serialize_head(&dto) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    assert_eq!(
        blob1, blob2,
        "two serializations of the same DTO must be bit-for-bit identical"
    );
}

/// Strategy for arbitrary `FlattenedAgentState` values.
fn flattened_agent_state_strategy() -> impl Strategy<Value = FlattenedAgentState> {
    prop_oneof![
        Just(FlattenedAgentState::Idle),
        any::<u64>().prop_map(|generation_id| FlattenedAgentState::Predicting { generation_id }),
        any::<u64>()
            .prop_map(|generation_id| FlattenedAgentState::ExecutingTools { generation_id }),
        any::<String>().prop_map(FlattenedAgentState::SubRoutine),
        Just(FlattenedAgentState::Failure),
    ]
}

/// Strategy for arbitrary `ToolCallDescriptor` values.
fn tool_call_descriptor_strategy() -> impl Strategy<Value = ToolCallDescriptor> {
    (
        any::<String>(),
        any::<String>(),
        any::<String>(),
        any::<Option<u64>>(),
    )
        .prop_map(
            |(tool_id, tool_name, arguments, timeout_ms)| ToolCallDescriptor {
                tool_id,
                tool_name,
                arguments,
                timeout_ms,
            },
        )
}

/// Strategy for arbitrary `ChatMessage` values.
fn chat_message_strategy() -> impl Strategy<Value = ChatMessage> {
    prop_oneof![
        any::<String>().prop_map(|content| ChatMessage::System { content }),
        any::<String>().prop_map(|content| ChatMessage::User { content }),
        (
            any::<String>(),
            any::<Option<String>>(),
            prop::collection::vec(tool_call_descriptor_strategy(), 0..5),
        )
            .prop_map(|(content, reasoning, tool_calls)| ChatMessage::Assistant {
                content,
                reasoning,
                tool_calls,
            }),
        (any::<String>(), any::<String>(), any::<String>()).prop_map(|(id, name, arguments)| {
            ChatMessage::ToolRequest {
                id,
                name,
                arguments,
            }
        }),
        (any::<String>(), any::<String>())
            .prop_map(|(id, content)| ChatMessage::ToolResult { id, content }),
    ]
}

/// Strategy for arbitrary `SessionHeadDTO` values.
fn session_head_dto_strategy() -> impl Strategy<Value = SessionHeadDTO> {
    (
        any::<String>(),
        any::<Option<String>>(),
        flattened_agent_state_strategy(),
        prop::collection::vec(flattened_agent_state_strategy(), 0..10),
        prop::collection::btree_map(
            any::<String>(),
            prop::collection::vec(any::<u8>(), 0..32),
            0..10,
        ),
        any::<u64>(),
        any::<u32>(),
    )
        .prop_map(
            |(
                id,
                parent_id,
                state,
                state_stack,
                extensions,
                persisted_msg_count,
                compaction_index,
            )| SessionHeadDTO {
                version: SessionSchemaVersion::V1,
                id,
                parent_id,
                state,
                state_stack,
                extensions,
                persisted_msg_count,
                compaction_index,
                checksum: None,
            },
        )
}

proptest! {
    #[test]
    fn prop_serialize_head_bit_for_bit_deterministic(dto in session_head_dto_strategy()) {
        let blob1 = match serialize_head(&dto) {
            Ok(b) => b,
            Err(_) => {
                prop_assert!(false, "first serialize_head failed");
                return Ok(());
            }
        };
        let blob2 = match serialize_head(&dto) {
            Ok(b) => b,
            Err(_) => {
                prop_assert!(false, "second serialize_head failed");
                return Ok(());
            }
        };
        prop_assert_eq!(blob1, blob2);
    }

    #[test]
    fn prop_session_head_extensions_order_independent(
        pairs in prop::collection::vec(
            (any::<String>(), prop::collection::vec(any::<u8>(), 0..32)),
            0..20,
        )
    ) {
        // Deduplicate by key so both maps end up with identical final values.
        let mut sorted = pairs;
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let mut unique: Vec<(String, Vec<u8>)> = Vec::new();
        for (k, v) in sorted {
            if unique.last().is_some_and(|(last_k, _)| last_k == &k) {
                continue;
            }
            unique.push((k, v));
        }

        let mut extensions1 = BTreeMap::new();
        let mut extensions2 = BTreeMap::new();
        for (k, v) in &unique {
            extensions1.insert(k.clone(), v.clone());
        }
        for (k, v) in unique.iter().rev() {
            extensions2.insert(k.clone(), v.clone());
        }

        let dto1 = SessionHeadDTO {
            version: SessionSchemaVersion::V1,
            id: "order-test".into(),
            parent_id: None,
            state: FlattenedAgentState::Idle,
            state_stack: vec![],
            extensions: extensions1,
            persisted_msg_count: 0,
            compaction_index: 0,
            checksum: None,
        };
        let dto2 = SessionHeadDTO {
            version: SessionSchemaVersion::V1,
            id: "order-test".into(),
            parent_id: None,
            state: FlattenedAgentState::Idle,
            state_stack: vec![],
            extensions: extensions2,
            persisted_msg_count: 0,
            compaction_index: 0,
            checksum: None,
        };

        let blob1 = match serialize_head(&dto1) {
            Ok(b) => b,
            Err(_) => {
                prop_assert!(false, "serialize dto1 failed");
                return Ok(());
            }
        };
        let blob2 = match serialize_head(&dto2) {
            Ok(b) => b,
            Err(_) => {
                prop_assert!(false, "serialize dto2 failed");
                return Ok(());
            }
        };
        prop_assert_eq!(blob1, blob2);
    }

    #[test]
    fn prop_chatmessage_serialization_bit_for_bit_deterministic(
        msg in chat_message_strategy()
    ) {
        let bytes1 = match rmp_serde::to_vec(&msg) {
            Ok(b) => b,
            Err(_) => {
                prop_assert!(false, "first rmp_serde serialization failed");
                return Ok(());
            }
        };
        let bytes2 = match rmp_serde::to_vec(&msg) {
            Ok(b) => b,
            Err(_) => {
                prop_assert!(false, "second rmp_serde serialization failed");
                return Ok(());
            }
        };
        prop_assert_eq!(bytes1, bytes2);
    }
}
