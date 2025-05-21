use super::common::LosslessNumber;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthorizationRequest {
    pub Client: String,
    pub Request: AuthorizationRequestData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthorizationRequestData {
    #[serde(rename = "Type")]
    pub request_type: i32,
    pub MetaReference: String,
    pub Action: i32,
    pub RedirectUri: String,
    pub OptionalCommerceId: Option<i32>,
    pub ClientInformation: ClientInformation,
    pub OptionalMetadata: Option<String>,
    pub LimitIssuers: Option<Vec<String>>,
    pub WebFormSettings: Option<serde_json::Value>,
    pub ExtendableInstrumentToken: Option<String>,
    pub DoNotUseCallback: Option<bool>,
    pub LimitBanks: Option<Vec<String>>,
    pub PromotionInfoIssuers: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientInformation {
    pub Name: String,
    pub Address: Option<String>,
    pub Email: Option<String>,
    pub Cellphone: Option<String>,
    pub Identification: Option<String>,
    pub IdentificationType: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaymentRequest {
    pub Client: String,
    pub Request: PaymentRequestData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaymentRequestData {
    pub ClientReferenceId: String,
    pub CurrencyId: i32,
    pub FinancialInclusion: FinancialInclusion,
    pub Installments: i32,
    pub Items: Vec<PaymentItem>,
    pub PaymentInstrumentInput: PaymentInstrumentInput,
    pub OptionalCommerceId: Option<i32>,
    pub LoyaltyProgramAmount: Option<LosslessNumber>,
    pub OptionalInstrumentFields: Option<HashMap<String, String>>,
    pub CommerceReserveExpirationInSeconds: Option<i32>,
    pub ThreeDSReferenceId: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FinancialInclusion {
    pub BilledAmount: LosslessNumber,
    pub InvoiceNumber: Option<i32>,
    pub TaxedAmount: LosslessNumber,
    #[serde(rename = "Type")]
    pub inclusion_type: i32,
    pub VATAmount: Option<LosslessNumber>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaymentItem {
    pub Amount: LosslessNumber,
    pub ClientItemReferenceId: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaymentInstrumentInput {
    pub InstrumentToken: String,
    pub UseExtendedClientCreditIfAvailable: bool,
    pub OptionalFields: Option<HashMap<String, String>>,
    pub InstrumentData: Option<InstrumentData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstrumentData {
    pub Issuer: Option<String>,
    #[serde(flatten)]
    pub additional_data: Option<HashMap<String, serde_json::Value>>,
}
