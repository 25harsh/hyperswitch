use actix_multipart::Field;
use common_utils::errors::CustomResult;
use error_stack::ResultExt;
use futures::TryStreamExt;
use hyperswitch_domain_models::router_response_types::disputes::FileInfo;

use crate::{
    core::{
        errors::{self, StorageErrorExt},
        payments, utils,
    },
    routes::SessionState,
    services,
    types::{self, api, domain, transformers::ForeignTryFrom},
};

pub async fn read_string(field: &mut Field) -> Option<String> {
    let bytes = field.try_next().await;
    if let Ok(Some(bytes)) = bytes {
        String::from_utf8(bytes.to_vec()).ok()
    } else {
        None
    }
}

pub async fn get_file_purpose(field: &mut Field) -> Option<api::FilePurpose> {
    let purpose = read_string(field).await;
    match purpose.as_deref() {
        Some("dispute_evidence") => Some(api::FilePurpose::DisputeEvidence),
        _ => None,
    }
}

pub async fn validate_file_upload(
    state: &SessionState,
    merchant_context: domain::MerchantContext,
    create_file_request: api::CreateFileRequest,
) -> CustomResult<(), errors::ApiErrorResponse> {
    //File Validation based on the purpose of file upload
    match create_file_request.purpose {
        api::FilePurpose::DisputeEvidence => {
            let dispute_id = &create_file_request
                .dispute_id
                .ok_or(errors::ApiErrorResponse::MissingDisputeId)?;
            let dispute = state
                .store
                .find_dispute_by_merchant_id_dispute_id(
                    merchant_context.get_merchant_account().get_id(),
                    dispute_id,
                )
                .await
                .to_not_found_response(errors::ApiErrorResponse::DisputeNotFound {
                    dispute_id: dispute_id.to_string(),
                })?;
            // Connector is not called for validating the file, connector_id can be passed as None safely
            let connector_data = api::ConnectorData::get_connector_by_name(
                &state.conf.connectors,
                &dispute.connector,
                api::GetToken::Connector,
                None,
            )?;

            let validation = connector_data.connector.validate_file_upload(
                create_file_request.purpose,
                create_file_request.file_size,
                create_file_request.file_type.clone(),
            );
            match validation {
                Ok(()) => Ok(()),
                Err(err) => match err.current_context() {
                    errors::ConnectorError::FileValidationFailed { reason } => {
                        Err(errors::ApiErrorResponse::FileValidationFailed {
                            reason: reason.to_string(),
                        }
                        .into())
                    }
                    //We are using parent error and ignoring this
                    _error => Err(err
                        .change_context(errors::ApiErrorResponse::InternalServerError)
                        .attach_printable("File validation failed"))?,
                },
            }
        }
    }
}

pub async fn delete_file_using_file_id(
    state: &SessionState,
    file_key: String,
    merchant_context: &domain::MerchantContext,
) -> CustomResult<(), errors::ApiErrorResponse> {
    let file_metadata_object = state
        .store
        .find_file_metadata_by_merchant_id_file_id(
            merchant_context.get_merchant_account().get_id(),
            &file_key,
        )
        .await
        .change_context(errors::ApiErrorResponse::FileNotFound)?;
    let (provider, provider_file_id) = match (
        file_metadata_object.file_upload_provider,
        file_metadata_object.provider_file_id,
        file_metadata_object.available,
    ) {
        (Some(provider), Some(provider_file_id), true) => (provider, provider_file_id),
        _ => Err(errors::ApiErrorResponse::FileNotAvailable)
            .attach_printable("File not available")?,
    };
    match provider {
        diesel_models::enums::FileUploadProvider::Router => state
            .file_storage_client
            .delete_file(&provider_file_id)
            .await
            .change_context(errors::ApiErrorResponse::InternalServerError),
        _ => Err(errors::ApiErrorResponse::FileProviderNotSupported {
            message: "Not Supported because provider is not Router".to_string(),
        }
        .into()),
    }
}

pub async fn retrieve_file_from_connector(
    state: &SessionState,
    file_metadata: diesel_models::file::FileMetadata,
    merchant_context: &domain::MerchantContext,
) -> CustomResult<Vec<u8>, errors::ApiErrorResponse> {
    let connector = &types::Connector::foreign_try_from(
        file_metadata
            .file_upload_provider
            .ok_or(errors::ApiErrorResponse::InternalServerError)
            .attach_printable("Missing file upload provider")?,
    )?
    .to_string();
    let connector_data = api::ConnectorData::get_connector_by_name(
        &state.conf.connectors,
        connector,
        api::GetToken::Connector,
        file_metadata.merchant_connector_id.clone(),
    )?;
    let connector_integration: services::BoxedFilesConnectorIntegrationInterface<
        api::Retrieve,
        types::RetrieveFileRequestData,
        types::RetrieveFileResponse,
    > = connector_data.connector.get_connector_integration();
    let router_data = utils::construct_retrieve_file_router_data(
        state,
        merchant_context,
        &file_metadata,
        connector,
    )
    .await
    .change_context(errors::ApiErrorResponse::InternalServerError)
    .attach_printable("Failed constructing the retrieve file router data")?;
    let response = services::execute_connector_processing_step(
        state,
        connector_integration,
        &router_data,
        payments::CallConnectorAction::Trigger,
        None,
    )
    .await
    .change_context(errors::ApiErrorResponse::InternalServerError)
    .attach_printable("Failed while calling retrieve file connector api")?;
    let retrieve_file_response =
        response
            .response
            .map_err(|err| errors::ApiErrorResponse::ExternalConnectorError {
                code: err.code,
                message: err.message,
                connector: connector.to_string(),
                status_code: err.status_code,
                reason: err.reason,
            })?;
    Ok(retrieve_file_response.file_data)
}

pub async fn retrieve_file_and_provider_file_id_from_file_id(
    state: &SessionState,
    file_id: Option<String>,
    merchant_context: &domain::MerchantContext,
    is_connector_file_data_required: api::FileDataRequired,
) -> CustomResult<FileInfo, errors::ApiErrorResponse> {
    match file_id {
        None => Ok(FileInfo {
            file_data: None,
            provider_file_id: None,
            file_type: None,
        }),
        Some(file_key) => {
            let file_metadata_object = state
                .store
                .find_file_metadata_by_merchant_id_file_id(
                    merchant_context.get_merchant_account().get_id(),
                    &file_key,
                )
                .await
                .change_context(errors::ApiErrorResponse::FileNotFound)?;
            let (provider, provider_file_id) = match (
                file_metadata_object.file_upload_provider,
                file_metadata_object.provider_file_id.clone(),
                file_metadata_object.available,
            ) {
                (Some(provider), Some(provider_file_id), true) => (provider, provider_file_id),
                _ => Err(errors::ApiErrorResponse::FileNotAvailable)
                    .attach_printable("File not available")?,
            };
            match provider {
                diesel_models::enums::FileUploadProvider::Router => Ok(FileInfo {
                    file_data: Some(
                        state
                            .file_storage_client
                            .retrieve_file(&provider_file_id)
                            .await
                            .change_context(errors::ApiErrorResponse::InternalServerError)?,
                    ),
                    provider_file_id: Some(provider_file_id),
                    file_type: Some(file_metadata_object.file_type),
                }),
                _ => {
                    let connector_file_data = match is_connector_file_data_required {
                        api::FileDataRequired::Required => Some(
                            retrieve_file_from_connector(
                                state,
                                file_metadata_object.clone(),
                                merchant_context,
                            )
                            .await?,
                        ),
                        api::FileDataRequired::NotRequired => None,
                    };
                    Ok(FileInfo {
                        file_data: connector_file_data,
                        provider_file_id: Some(provider_file_id),
                        file_type: Some(file_metadata_object.file_type),
                    })
                }
            }
        }
    }
}

#[cfg(feature = "v2")]
//Upload file to connector if it supports / store it in S3 and return file_upload_provider, provider_file_id accordingly
pub async fn upload_and_get_provider_provider_file_id_profile_id(
    state: &SessionState,
    merchant_context: &domain::MerchantContext,
    create_file_request: &api::CreateFileRequest,
    file_key: String,
) -> CustomResult<
    (
        String,
        api_models::enums::FileUploadProvider,
        Option<common_utils::id_type::ProfileId>,
        Option<common_utils::id_type::MerchantConnectorAccountId>,
    ),
    errors::ApiErrorResponse,
> {
    todo!()
}

#[cfg(feature = "v1")]
//Upload file to connector if it supports / store it in S3 and return file_upload_provider, provider_file_id accordingly
pub async fn upload_and_get_provider_provider_file_id_profile_id(
    state: &SessionState,
    merchant_context: &domain::MerchantContext,
    create_file_request: &api::CreateFileRequest,
    file_key: String,
) -> CustomResult<
    (
        String,
        api_models::enums::FileUploadProvider,
        Option<common_utils::id_type::ProfileId>,
        Option<common_utils::id_type::MerchantConnectorAccountId>,
    ),
    errors::ApiErrorResponse,
> {
    match create_file_request.purpose {
        api::FilePurpose::DisputeEvidence => {
            let dispute_id = create_file_request
                .dispute_id
                .clone()
                .ok_or(errors::ApiErrorResponse::MissingDisputeId)?;
            let dispute = state
                .store
                .find_dispute_by_merchant_id_dispute_id(
                    merchant_context.get_merchant_account().get_id(),
                    &dispute_id,
                )
                .await
                .to_not_found_response(errors::ApiErrorResponse::DisputeNotFound { dispute_id })?;
            let connector_data = api::ConnectorData::get_connector_by_name(
                &state.conf.connectors,
                &dispute.connector,
                api::GetToken::Connector,
                dispute.merchant_connector_id.clone(),
            )?;
            if connector_data.connector_name.supports_file_storage_module() {
                let payment_intent = state
                    .store
                    .find_payment_intent_by_payment_id_merchant_id(
                        &state.into(),
                        &dispute.payment_id,
                        merchant_context.get_merchant_account().get_id(),
                        merchant_context.get_merchant_key_store(),
                        merchant_context.get_merchant_account().storage_scheme,
                    )
                    .await
                    .change_context(errors::ApiErrorResponse::PaymentNotFound)?;

                let payment_attempt = state
                    .store
                    .find_payment_attempt_by_attempt_id_merchant_id(
                        &dispute.attempt_id,
                        merchant_context.get_merchant_account().get_id(),
                        merchant_context.get_merchant_account().storage_scheme,
                    )
                    .await
                    .change_context(errors::ApiErrorResponse::PaymentNotFound)?;

                let connector_integration: services::BoxedFilesConnectorIntegrationInterface<
                    api::Upload,
                    types::UploadFileRequestData,
                    types::UploadFileResponse,
                > = connector_data.connector.get_connector_integration();
                let router_data = utils::construct_upload_file_router_data(
                    state,
                    &payment_intent,
                    &payment_attempt,
                    merchant_context,
                    create_file_request,
                    &dispute.connector,
                    file_key,
                )
                .await
                .change_context(errors::ApiErrorResponse::InternalServerError)
                .attach_printable("Failed constructing the upload file router data")?;
                let response = services::execute_connector_processing_step(
                    state,
                    connector_integration,
                    &router_data,
                    payments::CallConnectorAction::Trigger,
                    None,
                )
                .await
                .change_context(errors::ApiErrorResponse::InternalServerError)
                .attach_printable("Failed while calling upload file connector api")?;

                let upload_file_response = response.response.map_err(|err| {
                    errors::ApiErrorResponse::ExternalConnectorError {
                        code: err.code,
                        message: err.message,
                        connector: dispute.connector.clone(),
                        status_code: err.status_code,
                        reason: err.reason,
                    }
                })?;
                Ok((
                    upload_file_response.provider_file_id,
                    api_models::enums::FileUploadProvider::foreign_try_from(
                        &connector_data.connector_name,
                    )?,
                    payment_intent.profile_id,
                    payment_attempt.merchant_connector_id,
                ))
            } else {
                state
                    .file_storage_client
                    .upload_file(&file_key, create_file_request.file.clone())
                    .await
                    .change_context(errors::ApiErrorResponse::InternalServerError)?;
                Ok((
                    file_key,
                    api_models::enums::FileUploadProvider::Router,
                    None,
                    None,
                ))
            }
        }
    }
}
