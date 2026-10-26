use crate::{Client, EtherscanError, Response, Result, block_number::BlockNumber};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[allow(missing_copy_implementations)]
pub struct BlockNumberByTimestamp {
    pub timestamp: u64,
    pub block_number: BlockNumber,
}

impl Client {
    /// Returns either (1) the oldest block since a particular timestamp occurred or (2) the newest
    /// block that occurred prior to that timestamp
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn foo(client: foundry_block_explorers::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // The newest block that occurred prior to 1 January 2020
    /// let block_number_before = client.get_block_by_timestamp(1577836800, "before");
    /// // The oldest block that occurred after 1 January 2020
    /// let block_number_after = client.get_block_by_timestamp(1577836800, "after");
    /// # Ok(()) }
    /// ```
    pub async fn get_block_by_timestamp(
        &self,
        timestamp: u64,
        closest: &str,
    ) -> Result<BlockNumberByTimestamp> {
        let query = self.create_query(
            "block",
            "getblocknobytime",
            HashMap::from([("timestamp", timestamp.to_string()), ("closest", closest.to_string())]),
        );
        let response: Response<String> = self.get_json(&query).await?;

        match response.status.as_str() {
            "0" => Err(EtherscanError::BlockNumberByTimestampFailed),
            "1" => Ok(BlockNumberByTimestamp {
                timestamp,
                block_number: parse_block_number(&response.result)?,
            }),
            err => Err(EtherscanError::BadStatusCode(err.to_string())),
        }
    }
}

/// Parses the `result` of a successful `getblocknobytime` response.
///
/// The value comes from the explorer, so a malformed one is reported as an error instead of
/// panicking.
fn parse_block_number(result: &str) -> Result<BlockNumber> {
    result.parse::<BlockNumber>().map_err(|err| {
        EtherscanError::Unknown(format!(
            "invalid block number {result:?} in getblocknobytime response: {err}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U64;

    #[test]
    fn parses_block_number_result() {
        assert_eq!(parse_block_number("12345").unwrap(), BlockNumber::Number(U64::from(12345)));
        assert_eq!(parse_block_number("0x3039").unwrap(), BlockNumber::Number(U64::from(12345)));
    }

    #[test]
    fn malformed_block_number_result_is_an_error() {
        for result in ["", "Error! No closest block found", "-1", "18446744073709551616"] {
            let err = parse_block_number(result).unwrap_err();
            assert!(
                matches!(&err, EtherscanError::Unknown(msg) if msg.contains("getblocknobytime")),
                "{result:?}: {err:?}"
            );
        }
    }
}
