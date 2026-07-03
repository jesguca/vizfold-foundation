## Model / Execution Target Compatibility

The current prototype treats model backends and execution targets as independently selectable. In a production version, not all model backends will be compatible with all execution targets. This should be represented with a compatibility table, likely `ModelExecutionTarget`, including status, notes, and target-specific configuration.