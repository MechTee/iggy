use crate::bytes_serializable::BytesSerializable;
use crate::command::CommandPayload;
use crate::error::IggyError;
use crate::identifier::Identifier;
use crate::messages::{MAX_HEADERS_SIZE, MAX_PAYLOAD_SIZE};
use crate::models::header;
use crate::models::header::{HeaderKey, HeaderValue};
use crate::validatable::Validatable;
use bytes::{BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use serde_with::base64::Base64;
use serde_with::serde_as;
use std::collections::HashMap;
use std::fmt::Display;
use std::str::FromStr;

const EMPTY_KEY_VALUE: Vec<u8> = vec![];

/// `SendMessages` command is used to send messages to a topic in a stream.
/// It has additional payload:
/// - `stream_id` - unique stream ID (numeric or name).
/// - `topic_id` - unique topic ID (numeric or name).
/// - `partitioning` - to which partition the messages should be sent - either provided by the client or calculated by the server.
/// - `messages` - collection of messages to be sent.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SendMessages {
    /// Unique stream ID (numeric or name).
    #[serde(skip)]
    pub stream_id: Identifier,
    /// Unique topic ID (numeric or name).
    #[serde(skip)]
    pub topic_id: Identifier,
    /// To which partition the messages should be sent - either provided by the client or calculated by the server.
    pub partitioning: Partitioning,
    /// Collection of messages to be sent.
    pub messages: Vec<Message>,
}

/// `Partitioning` is used to specify to which partition the messages should be sent.
/// It has the following kinds:
/// - `Balanced` - the partition ID is calculated by the server using the round-robin algorithm.
/// - `PartitionId` - the partition ID is provided by the client.
/// - `MessagesKey` - the partition ID is calculated by the server using the hash of the provided messages key.
#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Partitioning {
    /// The kind of partitioning.
    pub kind: PartitioningKind,
    #[serde(skip)]
    /// The length of the value payload.
    pub length: u8,
    #[serde_as(as = "Base64")]
    /// The binary value payload.
    pub value: Vec<u8>,
}

/// The single message to be sent. It has the following payload:
/// - `id` - unique message ID, if not specified by the client (has value = 0), it will be generated by the server.
/// - `length` - length of the payload.
/// - `payload` - binary message payload.
/// - `headers` - optional collection of headers.
#[serde_as]
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Message {
    /// Unique message ID, if not specified by the client (has value = 0), it will be generated by the server.
    #[serde(default = "default_message_id")]
    pub id: u128,
    #[serde(skip)]
    /// Length of the payload.
    pub length: u32,
    #[serde_as(as = "Base64")]
    /// Binary message payload.
    pub payload: Bytes,
    /// Optional collection of headers.
    pub headers: Option<HashMap<HeaderKey, HeaderValue>>,
}

/// `PartitioningKind` is an enum which specifies the kind of partitioning and is used by `Partitioning`.
#[derive(Debug, Serialize, Deserialize, PartialEq, Default, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum PartitioningKind {
    /// The partition ID is calculated by the server using the round-robin algorithm.
    #[default]
    Balanced,
    /// The partition ID is provided by the client.
    PartitionId,
    /// The partition ID is calculated by the server using the hash of the provided messages key.
    MessagesKey,
}

fn default_message_id() -> u128 {
    0
}

impl Default for SendMessages {
    fn default() -> Self {
        SendMessages {
            stream_id: Identifier::default(),
            topic_id: Identifier::default(),
            partitioning: Partitioning::default(),
            messages: vec![Message::default()],
        }
    }
}

impl Default for Partitioning {
    fn default() -> Self {
        Partitioning::balanced()
    }
}

impl Partitioning {
    /// Partition the messages using the balanced round-robin algorithm on the server.
    pub fn balanced() -> Self {
        Partitioning {
            kind: PartitioningKind::Balanced,
            length: 0,
            value: EMPTY_KEY_VALUE,
        }
    }

    /// Partition the messages using the provided partition ID.
    pub fn partition_id(partition_id: u32) -> Self {
        Partitioning {
            kind: PartitioningKind::PartitionId,
            length: 4,
            value: partition_id.to_le_bytes().to_vec(),
        }
    }

    /// Partition the messages using the provided messages key.
    pub fn messages_key(value: &[u8]) -> Result<Self, IggyError> {
        let length = value.len();
        if length == 0 || length > 255 {
            return Err(IggyError::InvalidCommand);
        }

        Ok(Partitioning {
            kind: PartitioningKind::MessagesKey,
            #[allow(clippy::cast_possible_truncation)]
            length: length as u8,
            value: value.to_vec(),
        })
    }

    /// Partition the messages using the provided messages key as str.
    pub fn messages_key_str(value: &str) -> Result<Self, IggyError> {
        Self::messages_key(value.as_bytes())
    }

    /// Partition the messages using the provided messages key as u32.
    pub fn messages_key_u32(value: u32) -> Self {
        Partitioning {
            kind: PartitioningKind::MessagesKey,
            length: 4,
            value: value.to_le_bytes().to_vec(),
        }
    }

    /// Partition the messages using the provided messages key as u64.
    pub fn messages_key_u64(value: u64) -> Self {
        Partitioning {
            kind: PartitioningKind::MessagesKey,
            length: 8,
            value: value.to_le_bytes().to_vec(),
        }
    }

    /// Partition the messages using the provided messages key as u128.
    pub fn messages_key_u128(value: u128) -> Self {
        Partitioning {
            kind: PartitioningKind::MessagesKey,
            length: 16,
            value: value.to_le_bytes().to_vec(),
        }
    }

    /// Create the partitioning from the provided partitioning.
    pub fn from_partitioning(partitioning: &Partitioning) -> Self {
        Partitioning {
            kind: partitioning.kind,
            length: partitioning.length,
            value: partitioning.value.clone(),
        }
    }

    /// Get the size of the partitioning in bytes.
    pub fn get_size_bytes(&self) -> u32 {
        2 + u32::from(self.length)
    }
}

impl CommandPayload for SendMessages {}

impl Validatable<IggyError> for SendMessages {
    fn validate(&self) -> Result<(), IggyError> {
        if self.messages.is_empty() {
            return Err(IggyError::InvalidMessagesCount);
        }

        let key_value_length = self.partitioning.value.len();
        if key_value_length > 255
            || (self.partitioning.kind != PartitioningKind::Balanced && key_value_length == 0)
        {
            return Err(IggyError::InvalidKeyValueLength);
        }

        let mut headers_size = 0;
        let mut payload_size = 0;
        for message in &self.messages {
            if let Some(headers) = &message.headers {
                for value in headers.values() {
                    headers_size += value.value.len() as u32;
                    if headers_size > MAX_HEADERS_SIZE {
                        return Err(IggyError::TooBigHeadersPayload);
                    }
                }
            }
            payload_size += message.payload.len() as u32;
            if payload_size > MAX_PAYLOAD_SIZE {
                return Err(IggyError::TooBigMessagePayload);
            }
        }

        if payload_size == 0 {
            return Err(IggyError::EmptyMessagePayload);
        }

        Ok(())
    }
}

impl PartitioningKind {
    /// Get the code of the partitioning kind.
    pub fn as_code(&self) -> u8 {
        match self {
            PartitioningKind::Balanced => 1,
            PartitioningKind::PartitionId => 2,
            PartitioningKind::MessagesKey => 3,
        }
    }

    /// Get the partitioning kind from the provided code.
    pub fn from_code(code: u8) -> Result<Self, IggyError> {
        match code {
            1 => Ok(PartitioningKind::Balanced),
            2 => Ok(PartitioningKind::PartitionId),
            3 => Ok(PartitioningKind::MessagesKey),
            _ => Err(IggyError::InvalidCommand),
        }
    }
}

impl Message {
    /// Create a new message with the optional ID, payload and headers.
    pub fn new(
        id: Option<u128>,
        payload: Bytes,
        headers: Option<HashMap<HeaderKey, HeaderValue>>,
    ) -> Self {
        Message {
            id: id.unwrap_or(0),
            #[allow(clippy::cast_possible_truncation)]
            length: payload.len() as u32,
            payload,
            headers,
        }
    }

    /// Get the size of the message in bytes.
    pub fn get_size_bytes(&self) -> u32 {
        // ID + Length + Payload + Headers
        16 + 4 + self.payload.len() as u32 + header::get_headers_size_bytes(&self.headers)
    }
}

impl Default for Message {
    fn default() -> Self {
        let payload = Bytes::from("hello world");
        Message {
            id: 0,
            length: payload.len() as u32,
            payload,
            headers: None,
        }
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}|{}", self.id, String::from_utf8_lossy(&self.payload))
    }
}

impl BytesSerializable for Partitioning {
    fn as_bytes(&self) -> Bytes {
        let mut bytes = BytesMut::with_capacity(2 + self.length as usize);
        bytes.put_u8(self.kind.as_code());
        bytes.put_u8(self.length);
        bytes.put_slice(&self.value);
        bytes.freeze()
    }

    fn from_bytes(bytes: Bytes) -> Result<Self, IggyError>
    where
        Self: Sized,
    {
        if bytes.len() < 3 {
            return Err(IggyError::InvalidCommand);
        }

        let kind = PartitioningKind::from_code(bytes[0])?;
        let length = bytes[1];
        let value = bytes[2..2 + length as usize].to_vec();
        if value.len() != length as usize {
            return Err(IggyError::InvalidCommand);
        }

        Ok(Partitioning {
            kind,
            length,
            value,
        })
    }
}

impl BytesSerializable for Message {
    fn as_bytes(&self) -> Bytes {
        let mut bytes = BytesMut::with_capacity(self.get_size_bytes() as usize);
        bytes.put_u128_le(self.id);
        if let Some(headers) = &self.headers {
            let headers_bytes = headers.as_bytes();
            bytes.put_u32_le(headers_bytes.len() as u32);
            bytes.put_slice(&headers_bytes);
        } else {
            bytes.put_u32_le(0);
        }
        bytes.put_u32_le(self.length);
        bytes.put_slice(&self.payload);
        bytes.freeze()
    }

    fn from_bytes(bytes: Bytes) -> Result<Self, IggyError> {
        if bytes.len() < 24 {
            return Err(IggyError::InvalidCommand);
        }

        let id = u128::from_le_bytes(bytes[..16].try_into()?);
        let headers_length = u32::from_le_bytes(bytes[16..20].try_into()?);
        let headers = if headers_length > 0 {
            Some(HashMap::from_bytes(
                bytes.slice(20..20 + headers_length as usize),
            )?)
        } else {
            None
        };

        let payload_length = u32::from_le_bytes(
            bytes[20 + headers_length as usize..24 + headers_length as usize].try_into()?,
        );
        if payload_length == 0 {
            return Err(IggyError::EmptyMessagePayload);
        }

        let payload = bytes.slice(
            24 + headers_length as usize..24 + headers_length as usize + payload_length as usize,
        );
        if payload.len() != payload_length as usize {
            return Err(IggyError::InvalidMessagePayloadLength);
        }

        Ok(Message {
            id,
            length: payload_length,
            payload,
            headers,
        })
    }
}

impl FromStr for Message {
    type Err = IggyError;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let id = default_message_id();
        let payload = Bytes::from(input.as_bytes().to_vec());
        let length = payload.len() as u32;
        if length == 0 {
            return Err(IggyError::EmptyMessagePayload);
        }

        Ok(Message {
            id,
            length,
            payload,
            headers: None,
        })
    }
}

impl BytesSerializable for SendMessages {
    fn as_bytes(&self) -> Bytes {
        let messages_size = self
            .messages
            .iter()
            .map(Message::get_size_bytes)
            .sum::<u32>();

        let key_bytes = self.partitioning.as_bytes();
        let stream_id_bytes = self.stream_id.as_bytes();
        let topic_id_bytes = self.topic_id.as_bytes();
        let mut bytes = BytesMut::with_capacity(
            stream_id_bytes.len() + topic_id_bytes.len() + key_bytes.len() + messages_size as usize,
        );
        bytes.put_slice(&stream_id_bytes);
        bytes.put_slice(&topic_id_bytes);
        bytes.put_slice(&key_bytes);
        for message in &self.messages {
            bytes.put_slice(&message.as_bytes());
        }

        bytes.freeze()
    }

    fn from_bytes(bytes: Bytes) -> Result<SendMessages, IggyError> {
        if bytes.len() < 11 {
            return Err(IggyError::InvalidCommand);
        }

        let mut position = 0;
        let stream_id = Identifier::from_bytes(bytes.clone())?;
        position += stream_id.get_size_bytes() as usize;
        let topic_id = Identifier::from_bytes(bytes.slice(position..))?;
        position += topic_id.get_size_bytes() as usize;
        let key = Partitioning::from_bytes(bytes.slice(position..))?;
        position += key.get_size_bytes() as usize;
        let messages_payloads = bytes.slice(position..);
        position = 0;
        let mut messages = Vec::new();
        while position < messages_payloads.len() {
            let message = Message::from_bytes(messages_payloads.slice(position..))?;
            position += message.get_size_bytes() as usize;
            messages.push(message);
        }

        let command = SendMessages {
            stream_id,
            topic_id,
            partitioning: key,
            messages,
        };
        command.validate()?;
        Ok(command)
    }
}

impl Display for SendMessages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}|{}|{}|{}",
            self.stream_id,
            self.topic_id,
            self.partitioning,
            self.messages
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<String>>()
                .join("|")
        )
    }
}

impl Display for Partitioning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            PartitioningKind::Balanced => write!(f, "{}|0", self.kind),
            PartitioningKind::PartitionId => write!(
                f,
                "{}|{}",
                self.kind,
                u32::from_le_bytes(self.value[..4].try_into().unwrap())
            ),
            PartitioningKind::MessagesKey => {
                write!(f, "{}|{}", self.kind, String::from_utf8_lossy(&self.value))
            }
        }
    }
}

impl Display for PartitioningKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartitioningKind::Balanced => write!(f, "balanced"),
            PartitioningKind::PartitionId => write!(f, "partition_id"),
            PartitioningKind::MessagesKey => write!(f, "messages_key"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_be_serialized_as_bytes() {
        let message_1 = Message::from_str("hello 1").unwrap();
        let message_2 = Message::new(Some(2), "hello 2".into(), None);
        let message_3 = Message::new(Some(3), "hello 3".into(), None);
        let messages = vec![message_1, message_2, message_3];
        let command = SendMessages {
            stream_id: Identifier::numeric(1).unwrap(),
            topic_id: Identifier::numeric(2).unwrap(),
            partitioning: Partitioning::partition_id(4),
            messages,
        };

        let bytes = command.as_bytes();

        let mut position = 0;
        let stream_id = Identifier::from_bytes(bytes.clone()).unwrap();
        position += stream_id.get_size_bytes() as usize;
        let topic_id = Identifier::from_bytes(bytes.slice(position..)).unwrap();
        position += topic_id.get_size_bytes() as usize;
        let key = Partitioning::from_bytes(bytes.slice(position..)).unwrap();
        position += key.get_size_bytes() as usize;
        let messages = bytes.slice(position..);
        let command_messages = command
            .messages
            .iter()
            .fold(BytesMut::new(), |mut bytes_mut, message| {
                bytes_mut.put(message.as_bytes());
                bytes_mut
            })
            .freeze();

        assert!(!bytes.is_empty());
        assert_eq!(stream_id, command.stream_id);
        assert_eq!(topic_id, command.topic_id);
        assert_eq!(key, command.partitioning);
        assert_eq!(messages, command_messages);
    }

    #[test]
    fn should_be_deserialized_from_bytes() {
        let stream_id = Identifier::numeric(1).unwrap();
        let topic_id = Identifier::numeric(2).unwrap();
        let key = Partitioning::partition_id(4);

        let message_1 = Message::from_str("hello 1").unwrap();
        let message_2 = Message::new(Some(2), "hello 2".into(), None);
        let message_3 = Message::new(Some(3), "hello 3".into(), None);
        let messages = [
            message_1.as_bytes(),
            message_2.as_bytes(),
            message_3.as_bytes(),
        ]
        .concat();

        let key_bytes = key.as_bytes();
        let stream_id_bytes = stream_id.as_bytes();
        let topic_id_bytes = topic_id.as_bytes();
        let current_position = stream_id_bytes.len() + topic_id_bytes.len() + key_bytes.len();
        let mut bytes = BytesMut::with_capacity(current_position);
        bytes.put_slice(&stream_id_bytes);
        bytes.put_slice(&topic_id_bytes);
        bytes.put_slice(&key_bytes);
        bytes.put_slice(&messages);
        let bytes = bytes.freeze();
        let command = SendMessages::from_bytes(bytes.clone());
        assert!(command.is_ok());

        let messages_payloads = bytes.slice(current_position..);
        let mut position = 0;
        let mut messages = Vec::new();
        while position < messages_payloads.len() {
            let message = Message::from_bytes(messages_payloads.slice(position..)).unwrap();
            position += message.get_size_bytes() as usize;
            messages.push(message);
        }

        let command = command.unwrap();
        assert_eq!(command.stream_id, stream_id);
        assert_eq!(command.topic_id, topic_id);
        assert_eq!(command.partitioning, key);
        for (index, message) in command.messages.iter().enumerate() {
            let command_message = &command.messages[index];
            assert_eq!(command_message.id, message.id);
            assert_eq!(command_message.length, message.length);
            assert_eq!(command_message.payload, message.payload);
        }
    }

    #[test]
    fn key_of_type_balanced_should_have_empty_value() {
        let key = Partitioning::balanced();
        assert_eq!(key.kind, PartitioningKind::Balanced);
        assert_eq!(key.length, 0);
        assert_eq!(key.value, EMPTY_KEY_VALUE);
        assert_eq!(
            PartitioningKind::from_code(1).unwrap(),
            PartitioningKind::Balanced
        );
    }

    #[test]
    fn key_of_type_partition_should_have_value_of_const_length_4() {
        let partition_id = 1234u32;
        let key = Partitioning::partition_id(partition_id);
        assert_eq!(key.kind, PartitioningKind::PartitionId);
        assert_eq!(key.length, 4);
        assert_eq!(key.value, partition_id.to_le_bytes());
        assert_eq!(
            PartitioningKind::from_code(2).unwrap(),
            PartitioningKind::PartitionId
        );
    }

    #[test]
    fn key_of_type_messages_key_should_have_value_of_dynamic_length() {
        let messages_key = "hello world";
        let key = Partitioning::messages_key_str(messages_key).unwrap();
        assert_eq!(key.kind, PartitioningKind::MessagesKey);
        assert_eq!(key.length, messages_key.len() as u8);
        assert_eq!(key.value, messages_key.as_bytes());
        assert_eq!(
            PartitioningKind::from_code(3).unwrap(),
            PartitioningKind::MessagesKey
        );
    }

    #[test]
    fn key_of_type_messages_key_that_has_length_0_should_fail() {
        let messages_key = "";
        let key = Partitioning::messages_key_str(messages_key);
        assert!(key.is_err());
    }

    #[test]
    fn key_of_type_messages_key_that_has_length_greater_than_255_should_fail() {
        let messages_key = "a".repeat(256);
        let key = Partitioning::messages_key_str(&messages_key);
        assert!(key.is_err());
    }
}
