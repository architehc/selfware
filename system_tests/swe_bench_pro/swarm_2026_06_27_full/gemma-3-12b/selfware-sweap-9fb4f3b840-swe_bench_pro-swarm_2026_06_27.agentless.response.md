### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
from keyword import iskeyword  # used in _is_fqcn
=======
from keyword import iskeyword

import re


def is_python_identifier(string):
    """Check if a string is a valid Python identifier."""
    return bool(re.match(r"^[a-zA-Z_][a-zA-Z0-9_]*$", string))


class AnsibleCollectionRef:

    @classmethod
    def is_valid_collection_name(cls, fqcn):
        """
        Validate Fully Qualified Collection Name (FQCN).

        The FQCN must be in the format <namespace>.<name>.
        Both <namespace> and <name> must be valid Python identifiers.
        """
        parts = fqcn.split('.')
        if len(parts) != 2:
            return False

        namespace, name = parts
        if not (is_python_identifier(namespace) and is_python_identifier(name)):
            return False

        if iskeyword(namespace) or iskeyword(name):
            return False

        return True
>>>>>>> REPLACE