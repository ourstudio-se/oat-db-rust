-- Drop the helper functions created in migration 008
DROP FUNCTION IF EXISTS migrate_data_type(text);
DROP FUNCTION IF EXISTS migrate_quantifier(text);
DROP FUNCTION IF EXISTS migrate_selection_type(text);
DROP FUNCTION IF EXISTS migrate_comparison_op(text);