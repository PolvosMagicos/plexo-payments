use super::common::LosslessNumber;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthorizationRequest {
    pub client: String,
    pub request: AuthorizationRequestData,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthorizationRequestData {
    #[serde(rename = "Type")]
    pub request_type: i32,

    pub meta_reference: String,
    pub action: i32,
    pub redirect_uri: String,

    pub optional_commerce_id: Option<i32>,
    pub client_information: ClientInformation,
    pub optional_metadata: Option<String>,
    pub limit_issuers: Option<Vec<String>>,
    pub web_form_settings: Option<serde_json::Value>,
    pub extendable_instrument_token: Option<String>,
    pub do_not_use_callback: Option<bool>,
    pub limit_banks: Option<Vec<String>>,
    pub promotion_info_issuers: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ClientInformation {
    pub name: String,
    pub address: Option<String>,
    pub email: Option<String>,
    pub cellphone: Option<String>,
    pub identification: Option<String>,
    pub identification_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PaymentRequest {
    pub client: String,
    pub request: PaymentRequestData,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PaymentRequestData {
    pub client_reference_id: String,
    pub currency_id: i32,
    pub financial_inclusion: FinancialInclusion,
    pub installments: i32,
    pub items: Vec<PaymentItem>,
    pub payment_instrument_input: PaymentInstrumentInput,

    pub optional_commerce_id: Option<i32>,
    pub loyalty_program_amount: Option<LosslessNumber>,
    pub optional_instrument_fields: Option<HashMap<String, String>>,
    pub commerce_reserve_expiration_in_seconds: Option<i32>,
    pub three_ds_reference_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FinancialInclusion {
    pub billed_amount: LosslessNumber,
    pub invoice_number: Option<i32>,
    pub taxed_amount: LosslessNumber,

    #[serde(rename = "Type")]
    pub inclusion_type: i32,

    pub vat_amount: Option<LosslessNumber>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PaymentItem {
    pub amount: LosslessNumber,
    pub client_item_reference_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PaymentInstrumentInput {
    pub instrument_token: String,
    pub use_extended_client_credit_if_available: bool,
    pub optional_fields: Option<HashMap<String, String>>,
    pub instrument_data: Option<InstrumentData>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InstrumentData {
    pub issuer: Option<String>,

    #[serde(flatten)]
    pub additional_data: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct StatusRequest {
    pub client: String,
    pub request: ReferenceRequest,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct ReferenceRequest {
    #[serde(rename = "Type")]
    pub reference_type: ReferenceType,

    pub meta_reference: String,
}

#[allow(clippy::enum_variant_names)]
#[repr(i32)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ReferenceType {
    PlexoTransactionId = 0,
    ClientPurchaseReferenceId = 1,
    ClientCancelReferenceId = 2,
    ClientReserveReferenceId = 3,
    ClientRefundReferenceId = 4,
}
