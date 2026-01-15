use std::borrow::Cow;

use redis::aio::ConnectionLike;
use serde_json::Value;

use crate::{
    errors::RepoError,
    runtime::{
        commands::{MutationCommand, MutationPlan},
        scripts::{
            ENTITY_DELETE_SCRIPT, ENTITY_DELETE_SCRIPT_BODY, ENTITY_GET_OR_CREATE_SCRIPT,
            ENTITY_GET_OR_CREATE_SCRIPT_BODY, ENTITY_MUTATION_SCRIPT, ENTITY_MUTATION_SCRIPT_BODY,
            ENTITY_PATCH_SCRIPT, ENTITY_PATCH_SCRIPT_BODY, ENTITY_UPSERT_SCRIPT, ENTITY_UPSERT_SCRIPT_BODY,
            RELATION_MUTATION_SCRIPT, RELATION_MUTATION_SCRIPT_BODY,
        },
    },
};

pub async fn execute_plan<C>(conn: &mut C, plan: &MutationPlan) -> Result<Vec<Value>, RepoError>
where
    C: ConnectionLike + Send,
{
    let mut responses = Vec::with_capacity(plan.commands.len());

    for command in &plan.commands {
        let (script, script_body) = match command {
            MutationCommand::UpsertEntity(_) => (&*ENTITY_MUTATION_SCRIPT, ENTITY_MUTATION_SCRIPT_BODY),
            MutationCommand::PatchEntity(_) => (&*ENTITY_PATCH_SCRIPT, ENTITY_PATCH_SCRIPT_BODY),
            MutationCommand::DeleteEntity(_) => (&*ENTITY_DELETE_SCRIPT, ENTITY_DELETE_SCRIPT_BODY),
            MutationCommand::MutateRelations(_) => (&*RELATION_MUTATION_SCRIPT, RELATION_MUTATION_SCRIPT_BODY),
            MutationCommand::Upsert(_) => (&*ENTITY_UPSERT_SCRIPT, ENTITY_UPSERT_SCRIPT_BODY),
            MutationCommand::GetOrCreate(_) => (&*ENTITY_GET_OR_CREATE_SCRIPT, ENTITY_GET_OR_CREATE_SCRIPT_BODY),
        };

        let payload = serde_json::to_string(command).map_err(|err| RepoError::Other {
            message: Cow::Owned(format!("failed to serialize command: {err}")),
        })?;

        let mut invocation = script.prepare_invoke();
        invocation.arg(payload);
        invocation.arg(script_body);
        let raw: String = invocation.invoke_async(conn).await.map_err(RepoError::from)?;

        let value: Value = serde_json::from_str(&raw).map_err(|err| RepoError::Other {
            message: Cow::Owned(format!("failed to parse lua response: {err}")),
        })?;

        if let Some(error) = value.get("error") {
            if let Some(code) = error.as_str() {
                match code {
                    "version_conflict" => {
                        let expected = value.get("expected").and_then(|v| v.as_u64());
                        let actual = value.get("actual").and_then(|v| v.as_u64());
                        return Err(RepoError::VersionConflict { expected, actual });
                    }
                    "entity_not_found" => {
                        let entity_id = value.get("entity_id").and_then(|v| v.as_str()).map(|s| s.to_string());
                        return Err(RepoError::NotFound { entity_id });
                    }
                    "unique_constraint_violation" => {
                        let fields = value
                            .get("fields")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let values = value
                            .get("values")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .map(|v| match v {
                                        Value::String(s) => s.clone(),
                                        other => other.to_string(),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let existing_entity_id = value
                            .get("existing_entity_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_default();
                        return Err(RepoError::UniqueConstraintViolation {
                            fields,
                            values,
                            existing_entity_id,
                        });
                    }
                    other => {
                        return Err(RepoError::Other {
                            message: Cow::Owned(other.to_string()),
                        });
                    }
                }
            }
            return Err(RepoError::Other {
                message: Cow::Owned("lua_error".to_string()),
            });
        }

        responses.push(value);
    }

    Ok(responses)
}

#[allow(async_fn_in_trait)]
pub trait MutationExecutor {
    async fn execute(&mut self, plan: MutationPlan) -> Result<Vec<Value>, RepoError>;
}

pub struct RedisExecutor<'a, C>
where
    C: ConnectionLike + Send,
{
    connection: &'a mut C,
}

impl<'a, C> RedisExecutor<'a, C>
where
    C: ConnectionLike + Send,
{
    pub fn new(connection: &'a mut C) -> Self {
        Self { connection }
    }
}

impl<'a, C> MutationExecutor for RedisExecutor<'a, C>
where
    C: ConnectionLike + Send,
{
    async fn execute(&mut self, plan: MutationPlan) -> Result<Vec<Value>, RepoError> {
        execute_plan(self.connection, &plan).await
    }
}
