#![allow(dead_code)]
mod transformers;

use std::fmt::Debug;

use common_utils::{crypto, ext_traits::ByteSliceExt};
use error_stack::{IntoReport, ResultExt};
use transformers as authorizedotnet;

use crate::{
    configs::settings,
    consts,
    core::errors::{self, CustomResult},
    headers,
    services::{self, logger},
    types::{
        self,
        api::{self, ConnectorCommon, ConnectorCommonExt},
    },
    utils::{self, BytesExt},
};

#[derive(Debug, Clone)]
pub struct Authorizedotnet;

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Authorizedotnet
where
    Self: services::ConnectorIntegration<Flow, Request, Response>,
{
    fn build_headers(
        &self,
        _req: &types::RouterData<Flow, Request, Response>,
        _connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, String)>, errors::ConnectorError> {
        // This connector does not require an auth header, the authentication details are sent in the request body
        Ok(vec![
            (
                headers::CONTENT_TYPE.to_string(),
                types::PaymentsAuthorizeType::get_content_type(self).to_string(),
            ),
            (headers::X_ROUTER.to_string(), "test".to_string()),
        ])
    }
}

impl ConnectorCommon for Authorizedotnet {
    fn id(&self) -> &'static str {
        "authorizedotnet"
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a settings::Connectors) -> &'a str {
        connectors.authorizedotnet.base_url.as_ref()
    }
}

impl api::Payment for Authorizedotnet {}
impl api::PaymentAuthorize for Authorizedotnet {}
impl api::PaymentSync for Authorizedotnet {}
impl api::PaymentVoid for Authorizedotnet {}
impl api::PaymentCapture for Authorizedotnet {}
impl api::PaymentSession for Authorizedotnet {}
impl api::ConnectorAccessToken for Authorizedotnet {}

impl
    services::ConnectorIntegration<
        api::Session,
        types::PaymentsSessionData,
        types::PaymentsResponseData,
    > for Authorizedotnet
{
    // Not Implemented (R)
}

impl
    services::ConnectorIntegration<
        api::AccessTokenAuth,
        types::AccessTokenRequestData,
        types::AccessToken,
    > for Authorizedotnet
{
    // Not Implemented (R)
}

impl api::PreVerify for Authorizedotnet {}
type Verify = dyn services::ConnectorIntegration<
    api::Verify,
    types::VerifyRequestData,
    types::PaymentsResponseData,
>;
impl
    services::ConnectorIntegration<
        api::Verify,
        types::VerifyRequestData,
        types::PaymentsResponseData,
    > for Authorizedotnet
{
    fn get_headers(
        &self,
        req: &types::RouterData<api::Verify, types::VerifyRequestData, types::PaymentsResponseData>,
        connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, String)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        _req: &types::RouterData<
            api::Verify,
            types::VerifyRequestData,
            types::PaymentsResponseData,
        >,
        connectors: &settings::Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(self.base_url(connectors).to_string())
    }

    fn get_request_body(
        &self,
        req: &types::RouterData<api::Verify, types::VerifyRequestData, types::PaymentsResponseData>,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        logger::debug!(request=?req);
        let connector_req = authorizedotnet::CreateTransactionRequest::try_from(req)?;
        let authorizedotnet_req =
            utils::Encode::<authorizedotnet::CreateTransactionRequest>::encode_to_string_of_json(
                &connector_req,
            )
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Some(authorizedotnet_req))
    }

    fn build_request(
        &self,
        req: &types::RouterData<api::Verify, types::VerifyRequestData, types::PaymentsResponseData>,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Ok(Some(
            services::RequestBuilder::new()
                .method(services::Method::Post)
                .url(&Verify::get_url(self, req, connectors)?)
                .headers(Verify::get_headers(self, req, connectors)?)
                .header(headers::X_ROUTER, "test")
                .body(Verify::get_request_body(self, req)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &types::RouterData<
            api::Verify,
            types::VerifyRequestData,
            types::PaymentsResponseData,
        >,
        res: types::Response,
    ) -> CustomResult<
        types::RouterData<api::Verify, types::VerifyRequestData, types::PaymentsResponseData>,
        errors::ConnectorError,
    > {
        use bytes::Buf;
        logger::debug!(authorizedotnetpayments_create_response=?res);

        // Handle the case where response bytes contains U+FEFF (BOM) character sent by connector
        let encoding = encoding_rs::UTF_8;
        let intermediate_response = encoding.decode_with_bom_removal(res.response.chunk());
        let intermediate_response =
            bytes::Bytes::copy_from_slice(intermediate_response.0.as_bytes());

        let response: authorizedotnet::AuthorizedotnetPaymentsResponse = intermediate_response
            .parse_struct("AuthorizedotnetPaymentsResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        types::RouterData::try_from((
            types::ResponseRouterData {
                response,
                data: data.clone(),
                http_code: res.status_code,
            },
            authorizedotnet::TransactionType::PaymentAuthOnly,
        ))
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(
        &self,
        res: types::Response,
    ) -> CustomResult<types::ErrorResponse, errors::ConnectorError> {
        logger::debug!(authorizedotnetpayments_create_error_response=?res);
        get_error_response(res)
    }
}

impl
    services::ConnectorIntegration<
        api::Capture,
        types::PaymentsCaptureData,
        types::PaymentsResponseData,
    > for Authorizedotnet
{
    fn get_headers(
        &self,
        req: &types::PaymentsCaptureRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, String)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn get_url(
        &self,
        _req: &types::PaymentsCaptureRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(self.base_url(connectors).to_string())
    }

    fn get_request_body(
        &self,
        req: &types::PaymentsCaptureRouterData,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        let connector_req = authorizedotnet::CreateTransactionRequest::try_from(req)?;
        let sync_request =
            utils::Encode::<authorizedotnet::CreateTransactionRequest>::encode_to_string_of_json(
                &connector_req,
            )
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Some(sync_request))
    }

    fn build_request(
        &self,
        req: &types::PaymentsCaptureRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        let request = services::RequestBuilder::new()
            .method(services::Method::Post)
            .url(&types::PaymentsCaptureType::get_url(self, req, connectors)?)
            .headers(types::PaymentsCaptureType::get_headers(
                self, req, connectors,
            )?)
            .body(types::PaymentsCaptureType::get_request_body(self, req)?)
            .build();
        Ok(Some(request))
    }

    fn handle_response(
        &self,
        data: &types::PaymentsCaptureRouterData,
        res: types::Response,
    ) -> CustomResult<types::PaymentsCaptureRouterData, errors::ConnectorError> {
        use bytes::Buf;

        // Handle the case where response bytes contains U+FEFF (BOM) character sent by connector
        let encoding = encoding_rs::UTF_8;
        let intermediate_response = encoding.decode_with_bom_removal(res.response.chunk());
        let intermediate_response =
            bytes::Bytes::copy_from_slice(intermediate_response.0.as_bytes());

        let response: authorizedotnet::AuthorizedotnetPaymentsResponse = intermediate_response
            .parse_struct("AuthorizedotnetPaymentsResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        types::RouterData::try_from((
            types::ResponseRouterData {
                response,
                data: data.clone(),
                http_code: res.status_code,
            },
            authorizedotnet::TransactionType::Capture,
        ))
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(
        &self,
        res: types::Response,
    ) -> CustomResult<types::ErrorResponse, errors::ConnectorError> {
        get_error_response(res)
    }
}

impl
    services::ConnectorIntegration<api::PSync, types::PaymentsSyncData, types::PaymentsResponseData>
    for Authorizedotnet
{
    fn get_headers(
        &self,
        req: &types::PaymentsSyncRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, String)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn get_url(
        &self,
        _req: &types::PaymentsSyncRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(self.base_url(connectors).to_string())
    }

    fn get_request_body(
        &self,
        req: &types::PaymentsSyncRouterData,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        let connector_req = authorizedotnet::AuthorizedotnetCreateSyncRequest::try_from(req)?;
        let sync_request =
            utils::Encode::<authorizedotnet::AuthorizedotnetCreateSyncRequest>::encode_to_string_of_json(
                &connector_req,
            )
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Some(sync_request))
    }

    fn build_request(
        &self,
        req: &types::PaymentsSyncRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        let request = services::RequestBuilder::new()
            .method(services::Method::Post)
            .url(&types::PaymentsSyncType::get_url(self, req, connectors)?)
            .headers(types::PaymentsSyncType::get_headers(self, req, connectors)?)
            .body(types::PaymentsSyncType::get_request_body(self, req)?)
            .build();
        Ok(Some(request))
    }

    fn handle_response(
        &self,
        data: &types::PaymentsSyncRouterData,
        res: types::Response,
    ) -> CustomResult<types::PaymentsSyncRouterData, errors::ConnectorError> {
        use bytes::Buf;

        // Handle the case where response bytes contains U+FEFF (BOM) character sent by connector
        let encoding = encoding_rs::UTF_8;
        let intermediate_response = encoding.decode_with_bom_removal(res.response.chunk());
        let intermediate_response =
            bytes::Bytes::copy_from_slice(intermediate_response.0.as_bytes());

        let response: authorizedotnet::AuthorizedotnetSyncResponse = intermediate_response
            .parse_struct("AuthorizedotnetSyncResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        types::RouterData::try_from(types::ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(
        &self,
        res: types::Response,
    ) -> CustomResult<types::ErrorResponse, errors::ConnectorError> {
        get_error_response(res)
    }
}

impl
    services::ConnectorIntegration<
        api::Authorize,
        types::PaymentsAuthorizeData,
        types::PaymentsResponseData,
    > for Authorizedotnet
{
    fn get_headers(
        &self,
        req: &types::PaymentsAuthorizeRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, String)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        _req: &types::PaymentsAuthorizeRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(self.base_url(connectors).to_string())
    }

    fn get_request_body(
        &self,
        req: &types::PaymentsAuthorizeRouterData,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        logger::debug!(request=?req);
        let connector_req = authorizedotnet::CreateTransactionRequest::try_from(req)?;
        let authorizedotnet_req =
            utils::Encode::<authorizedotnet::CreateTransactionRequest>::encode_to_string_of_json(
                &connector_req,
            )
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Some(authorizedotnet_req))
    }

    fn build_request(
        &self,
        req: &types::RouterData<
            api::Authorize,
            types::PaymentsAuthorizeData,
            types::PaymentsResponseData,
        >,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Ok(Some(
            services::RequestBuilder::new()
                .method(services::Method::Post)
                .url(&types::PaymentsAuthorizeType::get_url(
                    self, req, connectors,
                )?)
                .headers(types::PaymentsAuthorizeType::get_headers(
                    self, req, connectors,
                )?)
                .header(headers::X_ROUTER, "test")
                .body(types::PaymentsAuthorizeType::get_request_body(self, req)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &types::PaymentsAuthorizeRouterData,
        res: types::Response,
    ) -> CustomResult<types::PaymentsAuthorizeRouterData, errors::ConnectorError> {
        use bytes::Buf;
        logger::debug!(authorizedotnetpayments_create_response=?res);

        // Handle the case where response bytes contains U+FEFF (BOM) character sent by connector
        let encoding = encoding_rs::UTF_8;
        let intermediate_response = encoding.decode_with_bom_removal(res.response.chunk());
        let intermediate_response =
            bytes::Bytes::copy_from_slice(intermediate_response.0.as_bytes());

        let response: authorizedotnet::AuthorizedotnetPaymentsResponse = intermediate_response
            .parse_struct("AuthorizedotnetPaymentsResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        let transaction_type = match data.request.capture_method {
            Some(storage_models::enums::CaptureMethod::Manual) => {
                authorizedotnet::TransactionType::PaymentAuthOnly
            }
            _ => authorizedotnet::TransactionType::Payment,
        };

        types::RouterData::try_from((
            types::ResponseRouterData {
                response,
                data: data.clone(),
                http_code: res.status_code,
            },
            transaction_type,
        ))
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(
        &self,
        res: types::Response,
    ) -> CustomResult<types::ErrorResponse, errors::ConnectorError> {
        logger::debug!(authorizedotnetpayments_create_error_response=?res);
        get_error_response(res)
    }
}

impl
    services::ConnectorIntegration<
        api::Void,
        types::PaymentsCancelData,
        types::PaymentsResponseData,
    > for Authorizedotnet
{
    fn get_headers(
        &self,
        req: &types::PaymentsCancelRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, String)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        _req: &types::PaymentsCancelRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(self.base_url(connectors).to_string())
    }

    fn get_request_body(
        &self,
        req: &types::PaymentsCancelRouterData,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        let connector_req = authorizedotnet::CreateTransactionRequest::try_from(req)?;
        let authorizedotnet_req =
            utils::Encode::<authorizedotnet::CreateTransactionRequest>::encode_to_string_of_json(
                &connector_req,
            )
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Some(authorizedotnet_req))
    }
    fn build_request(
        &self,
        req: &types::PaymentsCancelRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Ok(Some(
            services::RequestBuilder::new()
                .method(services::Method::Post)
                .url(&types::PaymentsVoidType::get_url(self, req, connectors)?)
                .headers(types::PaymentsVoidType::get_headers(self, req, connectors)?)
                .header(headers::X_ROUTER, "test")
                .body(types::PaymentsVoidType::get_request_body(self, req)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &types::PaymentsCancelRouterData,
        res: types::Response,
    ) -> CustomResult<types::PaymentsCancelRouterData, errors::ConnectorError> {
        use bytes::Buf;

        // Handle the case where response bytes contains U+FEFF (BOM) character sent by connector
        let encoding = encoding_rs::UTF_8;
        let intermediate_response = encoding.decode_with_bom_removal(res.response.chunk());
        let intermediate_response =
            bytes::Bytes::copy_from_slice(intermediate_response.0.as_bytes());

        let response: authorizedotnet::AuthorizedotnetPaymentsResponse = intermediate_response
            .parse_struct("AuthorizedotnetPaymentsResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
        logger::debug!(authorizedotnetpayments_create_response=?response);

        types::RouterData::try_from((
            types::ResponseRouterData {
                response,
                data: data.clone(),
                http_code: res.status_code,
            },
            authorizedotnet::TransactionType::Void,
        ))
        .change_context(errors::ConnectorError::ResponseDeserializationFailed)
    }

    fn get_error_response(
        &self,
        res: types::Response,
    ) -> CustomResult<types::ErrorResponse, errors::ConnectorError> {
        get_error_response(res)
    }
}

impl api::Refund for Authorizedotnet {}
impl api::RefundExecute for Authorizedotnet {}
impl api::RefundSync for Authorizedotnet {}

impl services::ConnectorIntegration<api::Execute, types::RefundsData, types::RefundsResponseData>
    for Authorizedotnet
{
    fn get_headers(
        &self,
        req: &types::RefundsRouterData<api::Execute>,
        connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, String)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        _req: &types::RefundsRouterData<api::Execute>,
        connectors: &settings::Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(self.base_url(connectors).to_string())
    }

    fn get_request_body(
        &self,
        req: &types::RefundsRouterData<api::Execute>,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        logger::debug!(refund_request=?req);
        let connector_req = authorizedotnet::CreateTransactionRequest::try_from(req)?;
        let authorizedotnet_req =
            utils::Encode::<authorizedotnet::CreateTransactionRequest>::encode_to_string_of_json(
                &connector_req,
            )
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        println!("yyyyyy{:?}", authorizedotnet_req);
        Ok(Some(authorizedotnet_req))
    }

    fn build_request(
        &self,
        req: &types::RefundsRouterData<api::Execute>,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        let request = services::RequestBuilder::new()
            .method(services::Method::Post)
            .url(&types::RefundExecuteType::get_url(self, req, connectors)?)
            .headers(types::RefundExecuteType::get_headers(
                self, req, connectors,
            )?)
            .body(types::RefundExecuteType::get_request_body(self, req)?)
            .build();
        Ok(Some(request))
    }

    fn handle_response(
        &self,
        data: &types::RefundsRouterData<api::Execute>,
        res: types::Response,
    ) -> CustomResult<types::RefundsRouterData<api::Execute>, errors::ConnectorError> {
        use bytes::Buf;
        logger::debug!(response=?res);

        // Handle the case where response bytes contains U+FEFF (BOM) character sent by connector
        let encoding = encoding_rs::UTF_8;
        let intermediate_response = encoding.decode_with_bom_removal(res.response.chunk());
        let intermediate_response =
            bytes::Bytes::copy_from_slice(intermediate_response.0.as_bytes());

        let response: authorizedotnet::AuthorizedotnetRefundResponse = intermediate_response
            .parse_struct("AuthorizedotnetRefundResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
        logger::info!(response=?res);

        types::RouterData::try_from(types::ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(
        &self,
        res: types::Response,
    ) -> CustomResult<types::ErrorResponse, errors::ConnectorError> {
        get_error_response(res)
    }
}

impl services::ConnectorIntegration<api::RSync, types::RefundsData, types::RefundsResponseData>
    for Authorizedotnet
{
    fn get_headers(
        &self,
        req: &types::RefundsRouterData<api::RSync>,
        connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, String)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn get_url(
        &self,
        _req: &types::RefundsRouterData<api::RSync>,
        connectors: &settings::Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(self.base_url(connectors).to_string())
    }

    fn get_request_body(
        &self,
        req: &types::RefundsRouterData<api::RSync>,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        let connector_req = authorizedotnet::AuthorizedotnetCreateSyncRequest::try_from(req)?;
        let sync_request =
            utils::Encode::<authorizedotnet::AuthorizedotnetCreateSyncRequest>::encode_to_string_of_json(
                &connector_req,
            )
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Some(sync_request))
    }

    fn build_request(
        &self,
        req: &types::RefundsRouterData<api::RSync>,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        let request = services::RequestBuilder::new()
            .method(services::Method::Post)
            .url(&types::RefundSyncType::get_url(self, req, connectors)?)
            .headers(types::RefundSyncType::get_headers(self, req, connectors)?)
            .body(types::RefundSyncType::get_request_body(self, req)?)
            .build();
        Ok(Some(request))
    }

    fn handle_response(
        &self,
        data: &types::RefundsRouterData<api::RSync>,
        res: types::Response,
    ) -> CustomResult<types::RefundsRouterData<api::RSync>, errors::ConnectorError> {
        use bytes::Buf;

        // Handle the case where response bytes contains U+FEFF (BOM) character sent by connector
        let encoding = encoding_rs::UTF_8;
        let intermediate_response = encoding.decode_with_bom_removal(res.response.chunk());
        let intermediate_response =
            bytes::Bytes::copy_from_slice(intermediate_response.0.as_bytes());

        let response: authorizedotnet::AuthorizedotnetSyncResponse = intermediate_response
            .parse_struct("AuthorizedotnetSyncResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        types::RouterData::try_from(types::ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(
        &self,
        res: types::Response,
    ) -> CustomResult<types::ErrorResponse, errors::ConnectorError> {
        get_error_response(res)
    }
}

#[async_trait::async_trait]
impl api::IncomingWebhook for Authorizedotnet {
    fn get_webhook_source_verification_algorithm(
        &self,
        _headers: &actix_web::http::header::HeaderMap,
        _body: &[u8],
    ) -> CustomResult<Box<dyn crypto::VerifySignature + Send>, errors::ConnectorError> {
        Ok(Box::new(crypto::HmacSha512))
    }

    fn get_webhook_source_verification_signature(
        &self,
        headers: &actix_web::http::header::HeaderMap,
        _body: &[u8],
    ) -> CustomResult<Vec<u8>, errors::ConnectorError> {
        let security_header = headers
            .get("X-ANET-Signature")
            .map(|header_value| {
                header_value
                    .to_str()
                    .map(String::from)
                    .map_err(|_| errors::ConnectorError::WebhookSignatureNotFound)
                    .into_report()
            })
            .ok_or(errors::ConnectorError::WebhookSignatureNotFound)
            .into_report()??;

        let (_, sig_value) = security_header
            .split_once('=')
            .ok_or(errors::ConnectorError::WebhookSourceVerificationFailed)
            .into_report()?;
        Ok(sig_value.as_bytes().to_vec())
    }
    fn get_webhook_source_verification_message(
        &self,
        _headers: &actix_web::http::header::HeaderMap,
        body: &[u8],
        _merchant_id: &str,
        _secret: &[u8],
    ) -> CustomResult<Vec<u8>, errors::ConnectorError> {
        Ok(body.to_vec())
    }

    async fn get_webhook_source_verification_merchant_secret(
        &self,
        db: &dyn crate::db::StorageInterface,
        merchant_id: &str,
    ) -> CustomResult<Vec<u8>, errors::ConnectorError> {
        let key = format!("whsec_verification_{}_{}", self.id(), merchant_id);
        let secret = db
            .find_config_by_key(&key)
            .await
            .change_context(errors::ConnectorError::WebhookVerificationSecretNotFound)?;

        Ok(secret.config.into_bytes())
    }

    fn get_webhook_object_reference_id(
        &self,
        body: &[u8],
    ) -> CustomResult<String, errors::ConnectorError> {
        let details: authorizedotnet::AuthorizedotnetWebhookObjectId = body
            .parse_struct("AuthorizedotnetWebhookObjectId")
            .change_context(errors::ConnectorError::WebhookReferenceIdNotFound)?;

        Ok(details.payload.id)
    }

    fn get_webhook_event_type(
        &self,
        body: &[u8],
    ) -> CustomResult<api::IncomingWebhookEvent, errors::ConnectorError> {
        let details: authorizedotnet::AuthorizedotnetWebhookEventType = body
            .parse_struct("AuthorizedotnetWebhookEventType")
            .change_context(errors::ConnectorError::WebhookReferenceIdNotFound)?;

        Ok(match details.event_type.as_str() {
            "net.authorize.payment.authorization.created"
            | "net.authorize.payment.priorAuthCapture.created"
            | "net.authorize.payment.authcapture.created" => {
                api::IncomingWebhookEvent::PaymentIntentSuccess
            }
            _ => Err(errors::ConnectorError::WebhookEventTypeNotFound).into_report()?,
        })
    }

    fn get_webhook_resource_object(
        &self,
        body: &[u8],
    ) -> CustomResult<serde_json::Value, errors::ConnectorError> {
        let details: authorizedotnet::AuthorizedotnetWebhookObjectResource = body
            .parse_struct("AuthorizedotnetWebhookObjectResource")
            .change_context(errors::ConnectorError::WebhookReferenceIdNotFound)?;
        let sync_response = authorizedotnet::AuthorizedotnetSyncResponse::from(details);
        let payload = serde_json::to_value(sync_response)
            .into_report()
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        Ok(payload)
    }
}

impl services::ConnectorRedirectResponse for Authorizedotnet {}

#[inline]
fn get_error_response(
    types::Response {
        response,
        status_code,
    }: types::Response,
) -> CustomResult<types::ErrorResponse, errors::ConnectorError> {
    let response: authorizedotnet::AuthorizedotnetPaymentsResponse = response
        .parse_struct("AuthorizedotnetPaymentsResponse")
        .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

    logger::info!(response=?response);

    Ok(response
        .transaction_response
        .errors
        .and_then(|errors| {
            errors.into_iter().next().map(|error| types::ErrorResponse {
                code: error.error_code,
                message: error.error_text,
                reason: None,
                status_code,
            })
        })
        .unwrap_or_else(|| types::ErrorResponse {
            code: consts::NO_ERROR_CODE.to_string(),
            message: consts::NO_ERROR_MESSAGE.to_string(),
            reason: None,
            status_code,
        }))
}
