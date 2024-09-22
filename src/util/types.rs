use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy, Default)]
#[serde(transparent)]
pub struct ByteArray32(pub [u8; 32]);

// impl <'de> Deserialize<'de> for ByteArray32 {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>
//     {
//         // 1. hex str
//         // 2. base64 str
//         // 3. base58 str
//         // 4. number array
//
//         // visitor
//         struct ByteArray32Visitor;
//
//         impl<'de> Visitor<'de> for ByteArray32Visitor {
//             type Value = ByteArray32;
//
//             fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//                 formatter.write_str("a 32-byte array")
//             }
//
//             fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
//             where
//                 E: de::Error,
//             {
//                 if value.len() == 64 {
//                     let mut bytes = [0u8; 32];
//                     hex::decode_to_slice(value, &mut bytes).map_err(de::Error::custom)?;
//                     Ok(ByteArray32(bytes))
//                 } else {
//                     Err(de::Error::custom("invalid length"))
//                 }
//             }
//
//             fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
//             where
//                 E: de::Error,
//             {
//                 if value.len() == 32 {
//                     let mut bytes = [0u8; 32];
//                     bytes.copy_from_slice(value);
//                     Ok(ByteArray32(bytes))
//                 } else {
//                     Err(de::Error::custom("invalid length"))
//                 }
//             }
//         }
//
//         deserializer.deserialize_str(ByteArray32Visitor)
//     }
// }
//
impl ByteArray32 {
    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }
}