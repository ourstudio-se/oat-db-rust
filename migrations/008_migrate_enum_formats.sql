-- Migration to update enum formats from PascalCase/SCREAMING_SNAKE_CASE to kebab-case

-- First, let's create a helper function to update DataType values
CREATE OR REPLACE FUNCTION migrate_data_type(old_value text) RETURNS text AS $$
BEGIN
    RETURN CASE old_value
        WHEN 'String' THEN 'string'
        WHEN 'Number' THEN 'number'
        WHEN 'Boolean' THEN 'boolean'
        WHEN 'Date' THEN 'date'
        WHEN 'Object' THEN 'object'
        WHEN 'Array' THEN 'array'
        WHEN 'StringList' THEN 'string-list'
        ELSE old_value
    END;
END;
$$ LANGUAGE plpgsql;

-- Create a helper function to update Quantifier values
CREATE OR REPLACE FUNCTION migrate_quantifier(old_value text) RETURNS text AS $$
BEGIN
    RETURN CASE old_value
        WHEN 'EXACTLY' THEN 'exactly'
        WHEN 'AT_LEAST' THEN 'at-least'
        WHEN 'AT_MOST' THEN 'at-most'
        WHEN 'BETWEEN' THEN 'between'
        WHEN 'ANY' THEN 'any'
        ELSE old_value
    END;
END;
$$ LANGUAGE plpgsql;

-- Create a helper function to update SelectionType values
CREATE OR REPLACE FUNCTION migrate_selection_type(old_value text) RETURNS text AS $$
BEGIN
    RETURN CASE old_value
        WHEN 'Manual' THEN 'manual'
        WHEN 'All' THEN 'all'
        WHEN 'Query' THEN 'query'
        ELSE old_value
    END;
END;
$$ LANGUAGE plpgsql;

-- Create a helper function to update ComparisonOp values
CREATE OR REPLACE FUNCTION migrate_comparison_op(old_value text) RETURNS text AS $$
BEGIN
    RETURN CASE old_value
        WHEN 'EQ' THEN 'eq'
        WHEN 'NE' THEN 'ne'
        WHEN 'GT' THEN 'gt'
        WHEN 'GTE' THEN 'gte'
        WHEN 'LT' THEN 'lt'
        WHEN 'LTE' THEN 'lte'
        ELSE old_value
    END;
END;
$$ LANGUAGE plpgsql;

-- Update commits table
UPDATE commits
SET data = data || jsonb_build_object(
    'schema_data', jsonb_build_object(
        'id', data->'schema_data'->>'id',
        'description', data->'schema_data'->'description',
        'classes', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', class_elem->>'id',
                    'name', class_elem->>'name',
                    'description', class_elem->'description',
                    'properties', COALESCE((
                        SELECT jsonb_agg(
                            jsonb_build_object(
                                'id', prop_elem->>'id',
                                'name', prop_elem->>'name',
                                'data_type', migrate_data_type(prop_elem->>'data_type'),
                                'required', prop_elem->'required',
                                'value', prop_elem->'value'
                            )
                        )
                        FROM jsonb_array_elements(class_elem->'properties') AS prop_elem
                    ), '[]'::jsonb),
                    'relationships', COALESCE((
                        SELECT jsonb_agg(
                            jsonb_build_object(
                                'id', rel_elem->>'id',
                                'name', rel_elem->>'name',
                                'targets', rel_elem->'targets',
                                'quantifier', migrate_quantifier(rel_elem->>'quantifier'),
                                'universe', rel_elem->'universe',
                                'selection', migrate_selection_type(rel_elem->>'selection'),
                                'default_pool', CASE
                                    WHEN rel_elem->'default_pool'->>'mode' IS NOT NULL THEN
                                        CASE
                                            WHEN rel_elem->'default_pool'->'filter'->'conditions' IS NOT NULL THEN
                                                jsonb_build_object(
                                                    'mode', rel_elem->'default_pool'->>'mode',
                                                    'type', rel_elem->'default_pool'->'type',
                                                    'where', jsonb_build_object(
                                                        'conditions', (
                                                            SELECT jsonb_agg(
                                                                jsonb_build_object(
                                                                    'property', cond_elem->>'property',
                                                                    'op', migrate_comparison_op(cond_elem->>'op'),
                                                                    'value', cond_elem->'value'
                                                                )
                                                            )
                                                            FROM jsonb_array_elements(rel_elem->'default_pool'->'filter'->'conditions') AS cond_elem
                                                        )
                                                    )
                                                )
                                            ELSE rel_elem->'default_pool'
                                        END
                                    ELSE rel_elem->'default_pool'
                                END
                            )
                        )
                        FROM jsonb_array_elements(class_elem->'relationships') AS rel_elem
                    ), '[]'::jsonb),
                    'derived', COALESCE((
                        SELECT jsonb_agg(
                            jsonb_build_object(
                                'id', der_elem->>'id',
                                'name', der_elem->>'name',
                                'data_type', migrate_data_type(der_elem->>'data_type'),
                                'expr', der_elem->'expr'
                            )
                        )
                        FROM jsonb_array_elements(class_elem->'derived') AS der_elem
                    ), '[]'::jsonb)
                )
            )
            FROM jsonb_array_elements(data->'schema_data'->'classes') AS class_elem
        )
    ),
    'instances', data->'instances'
)
WHERE data->'schema_data'->'classes' IS NOT NULL;

-- Update working_commits table
UPDATE working_commits
SET schema_data = jsonb_build_object(
    'id', schema_data->>'id',
    'description', schema_data->'description',
    'classes', (
        SELECT jsonb_agg(
            jsonb_build_object(
                'id', class_elem->>'id',
                'name', class_elem->>'name',
                'description', class_elem->'description',
                'properties', COALESCE((
                    SELECT jsonb_agg(
                        jsonb_build_object(
                            'id', prop_elem->>'id',
                            'name', prop_elem->>'name',
                            'data_type', migrate_data_type(prop_elem->>'data_type'),
                            'required', prop_elem->'required',
                            'value', prop_elem->'value'
                        )
                    )
                    FROM jsonb_array_elements(class_elem->'properties') AS prop_elem
                ), '[]'::jsonb),
                'relationships', COALESCE((
                    SELECT jsonb_agg(
                        jsonb_build_object(
                            'id', rel_elem->>'id',
                            'name', rel_elem->>'name',
                            'targets', rel_elem->'targets',
                            'quantifier', migrate_quantifier(rel_elem->>'quantifier'),
                            'universe', rel_elem->'universe',
                            'selection', migrate_selection_type(rel_elem->>'selection'),
                            'default_pool', CASE
                                WHEN rel_elem->'default_pool'->>'mode' IS NOT NULL THEN
                                    CASE
                                        WHEN rel_elem->'default_pool'->'where'->'conditions' IS NOT NULL THEN
                                            jsonb_build_object(
                                                'mode', rel_elem->'default_pool'->>'mode',
                                                'type', rel_elem->'default_pool'->'type',
                                                'where', jsonb_build_object(
                                                    'conditions', (
                                                        SELECT jsonb_agg(
                                                            jsonb_build_object(
                                                                'property', cond_elem->>'property',
                                                                'op', migrate_comparison_op(cond_elem->>'op'),
                                                                'value', cond_elem->'value'
                                                            )
                                                        )
                                                        FROM jsonb_array_elements(rel_elem->'default_pool'->'where'->'conditions') AS cond_elem
                                                    )
                                                )
                                            )
                                        ELSE rel_elem->'default_pool'
                                    END
                                ELSE rel_elem->'default_pool'
                            END
                        )
                    )
                    FROM jsonb_array_elements(class_elem->'relationships') AS rel_elem
                ), '[]'::jsonb),
                'derived', COALESCE((
                    SELECT jsonb_agg(
                        jsonb_build_object(
                            'id', der_elem->>'id',
                            'name', der_elem->>'name',
                            'data_type', migrate_data_type(der_elem->>'data_type'),
                            'expr', der_elem->'expr'
                        )
                    )
                    FROM jsonb_array_elements(class_elem->'derived') AS der_elem
                ), '[]'::jsonb)
            )
        )
        FROM jsonb_array_elements(schema_data->'classes') AS class_elem
    )
)
WHERE schema_data->'classes' IS NOT NULL;

-- Drop the helper functions
DROP FUNCTION IF EXISTS migrate_data_type(text);
DROP FUNCTION IF EXISTS migrate_quantifier(text);
DROP FUNCTION IF EXISTS migrate_selection_type(text);
DROP FUNCTION IF EXISTS migrate_comparison_op(text);