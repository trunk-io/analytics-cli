import json
import pathlib

# trunk-ignore(bandit/B404)
import subprocess
from enum import Enum
from typing import Any, Dict, Optional, Set


class FdKind(Enum):
    OBJECT = "object"
    VALUE = "value"
    ARRAY = "array"


class FdValue(Enum):
    BOOL = "Bool"
    DATA = "Data"
    DATE = "Date"
    DOUBLE = "Double"
    INT = "Int"
    INT16 = "Int16"
    INT32 = "Int32"
    INT64 = "Int64"
    INT8 = "Int8"
    STRING = "String"
    UINT16 = "UInt16"
    UINT32 = "UInt32"
    UINT64 = "UInt64"
    UINT8 = "UInt8"
    URL = "URL"


class FdWrapperType(Enum):
    ARRAY = "Array"
    OPTIONAL = "Optional"


BAD_FD_TYPES = [
    # NOTE: These types are problematic to codegen for because they have a property `type` that
    # conflicts with another property `_type`.
    #
    # https://github.com/oxidecomputer/typify/issues/638
    "ActivityLogMessage",
    "ActivityLogAnalyzerWarningMessage",
    "ActivityLogAnalyzerResultMessage",
    # NOTE: This type isn't given in the format description. Seems to be a bug in Apple's schema.
    "SchemaSerializable",
]


def convert_fd_value_to_json_schema_format() -> Dict[str, Any]:
    return {
        "type": "object",
        "properties": {
            "_type": {"type": "object"},
            "_value": {
                "type": "string",
            },
        },
        "required": ["_value"],
    }


def convert_fd_array_to_json_schema_format(
    items_json_schema_object: Any,
) -> Dict[str, Any]:
    return {
        "type": "object",
        "properties": {
            "_type": {"type": "object"},
            "_values": {
                "type": "array",
                "items": items_json_schema_object,
            },
        },
        "required": [
            "_values",
        ],
    }


def convert_fd_object_to_json_schema_format(
    fd_type: Any, fd_types: Any, fd_types_inheritance_hierarchy: Dict[str, list[str]]
) -> Dict[str, Any]:
    json_schema_object_properties: Dict[str, Any] = {
        "_type": {"type": "object"},
    }
    json_schema_object_required_properties: Set[str] = set()

    if "supertype" in fd_type["type"]:
        fd_type_supertype_name = fd_type["type"]["supertype"]
        supertype_fd_type = next(
            st for st in fd_types if st["type"]["name"] == fd_type_supertype_name
        )
        supertype_json_schema_object = convert_fd_object_to_json_schema_format(
            supertype_fd_type, fd_types, fd_types_inheritance_hierarchy
        )
        if "properties" in supertype_json_schema_object:
            json_schema_object_properties |= supertype_json_schema_object["properties"]
        else:
            json_schema_object_properties |= supertype_json_schema_object["oneOf"][-1][
                "properties"
            ]
        for required_property in supertype_json_schema_object.get("required", []):
            json_schema_object_required_properties.add(required_property)

    for fd_property in fd_type["properties"]:
        fd_property_name = fd_property["name"]
        fd_property_type = fd_property["type"]

        is_array = False
        is_required = True
        if fd_property_type in FdWrapperType:
            match FdWrapperType(fd_property_type):
                case FdWrapperType.ARRAY:
                    is_array = True
                case FdWrapperType.OPTIONAL:
                    pass
            # NOTE: Even arrays can be optional when xcresulttool formats JSON.
            is_required = False
            fd_property_type = fd_property["wrappedType"]
        # NOTE: This check must be after updating the `fd_property_type` variable.
        if fd_property_type in BAD_FD_TYPES:
            continue

        if fd_property_type in FdValue:
            # NOTE: Even values can be optional when xcresulttool formats JSON.
            is_required = False
        if is_required:
            json_schema_object_required_properties.add(fd_property_name)

        json_schema_object: Dict[str, Any] = {"$ref": f"#/$defs/{fd_property_type}"}

        if is_array:
            json_schema_object = convert_fd_array_to_json_schema_format(
                json_schema_object
            )

        json_schema_object_properties[fd_property_name] = json_schema_object

    fd_type_name = fd_type["type"]["name"]
    if fd_type_name == "ActionTestPlanRunSummaries":
        json_schema_object_properties["failureSummaries"] = {
            "type": "object",
            "properties": {
                "_type": {"type": "object"},
                "_values": {
                    "type": "array",
                    "items": {"$ref": "#/$defs/ActionTestFailureSummary"},
                },
            },
            "required": ["_values"],
        }

    json_schema_def: Dict[str, Any] = {
        "type": "object",
        "properties": json_schema_object_properties,
        "additionalProperties": fd_type_name == "ActionTestPlanRunSummaries",
    }
    if len(json_schema_object_required_properties) > 0:
        json_schema_def["required"] = list(json_schema_object_required_properties)

    if fd_type_name in fd_types_inheritance_hierarchy:
        json_schema_def = {
            "type": "object",
            "oneOf": [
                {"$ref": f"#/$defs/{subtype_name}"}
                for subtype_name in fd_types_inheritance_hierarchy[fd_type_name]
            ]
            + [
                # NOTE: `supertype`s are more generic than their sub-types, so they should be
                # ordered last in the `oneOf` list.
                json_schema_def
            ],
        }

    return json_schema_def


def convert_fd_type_to_json_schema_format(
    fd_type: Any, fd_types: Any, fd_types_inheritance_hierarchy: Dict[str, list[str]]
) -> Optional[Dict[str, Any]]:
    match FdKind(fd_type["kind"]):
        case FdKind.OBJECT:
            return convert_fd_object_to_json_schema_format(
                fd_type, fd_types, fd_types_inheritance_hierarchy
            )
        case FdKind.VALUE:
            return convert_fd_value_to_json_schema_format()
        case FdKind.ARRAY:
            # Supporting arrays of arrays is unnecessary because arrays are primitives in JSONSchema.
            # In the format description they are defined as a part of the schema and we can ignore them.
            pass


def traverse_fd_types_inheritance_hierarchy(
    fd_type: Any,
    fd_types: Any,
    fd_types_inheritance_hierarchy: Dict[str, Dict[str, int]],
) -> Optional[set[str]]:
    if "supertype" not in fd_type["type"]:
        return

    fd_type_name = fd_type["type"]["name"]
    fd_type_supertype_name = fd_type["type"]["supertype"]

    fd_supertype = next(
        st for st in fd_types if st["type"]["name"] == fd_type_supertype_name
    )
    node = traverse_fd_types_inheritance_hierarchy(
        fd_supertype, fd_types, fd_types_inheritance_hierarchy
    )

    if fd_type_supertype_name not in fd_types_inheritance_hierarchy:
        fd_types_inheritance_hierarchy[fd_type_supertype_name] = {}
    fd_types_inheritance_hierarchy[fd_type_supertype_name][fd_type_name] = len(
        fd_type["properties"]
    )

    return node


def order_fd_types_by_inheritance_hierarchy_by_number_of_properties(
    fd_types_inheritance_hierarchy: Dict[str, Dict[str, int]]
) -> Dict[str, list[str]]:
    fd_types_inheritance_hierarchy_orderd_by_number_of_properties: Dict[
        str, list[str]
    ] = {}
    for (
        fd_supertype_name,
        fd_subtype_name_and_property_count,
    ) in fd_types_inheritance_hierarchy.items():
        fd_types_inheritance_hierarchy_orderd_by_number_of_properties[
            fd_supertype_name
        ] = list()
        for fd_subtype_name, _ in sorted(
            fd_subtype_name_and_property_count.items(), key=lambda item: item[1]
        ):
            if (
                fd_subtype_name
                not in fd_types_inheritance_hierarchy_orderd_by_number_of_properties
            ):
                fd_types_inheritance_hierarchy_orderd_by_number_of_properties[
                    fd_supertype_name
                ].append(fd_subtype_name)
    return fd_types_inheritance_hierarchy_orderd_by_number_of_properties


def main():
    # trunk-ignore(bandit/B603,bandit/B607)
    result = subprocess.run(
        [
            "xcrun",
            "xcresulttool",
            "formatDescription",
            "get",
            "--format",
            "json",
            "--legacy",
        ],
        capture_output=True,
        text=True,
    )
    format_description = json.loads(result.stdout)

    json_schema_defs: Dict[str, Any] = {}
    fd_types = [
        fd_type
        for fd_type in format_description["types"]
        if fd_type["type"]["name"] not in BAD_FD_TYPES
    ]
    fd_types_inheritance_hierarchy: Dict[str, Dict[str, int]] = {}
    for fd_type in fd_types:
        traverse_fd_types_inheritance_hierarchy(
            fd_type, fd_types, fd_types_inheritance_hierarchy
        )
    fd_types_inheritance_hierarchy_orderd_by_number_of_properties: Dict[
        str, list[str]
    ] = order_fd_types_by_inheritance_hierarchy_by_number_of_properties(
        fd_types_inheritance_hierarchy
    )
    for fd_type in fd_types:
        json_schema_object = convert_fd_type_to_json_schema_format(
            fd_type,
            fd_types,
            fd_types_inheritance_hierarchy_orderd_by_number_of_properties,
        )
        if json_schema_object:
            fd_type_name = fd_type["type"]["name"]
            json_schema_defs[fd_type_name] = json_schema_object

    json_schema: Dict[str, Any] = {
        "$defs": json_schema_defs,
    }

    pathlib.Path(__file__).parent.resolve().joinpath(
        "./xcrun-xcresulttool-formatDescription-get---format-json---legacy-json-schema.json"
    ).write_text(json.dumps(json_schema, indent=2))


if __name__ == "__main__":
    main()
