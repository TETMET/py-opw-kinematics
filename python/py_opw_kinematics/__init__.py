import sys as _sys

try:
    from ._internal import BaseConfig, KinematicModel, Robot, ToolConfig
except ImportError as _e:
    raise ImportError(
        f"py_opw_kinematics: failed to load native extension '_internal'.\n"
        f"  Reason  : {_e}\n"
        f"  Python  : {_sys.version}\n"
        f"  Platform: {_sys.platform}"
    ) from _e
finally:
    del _sys

__all__ = ["BaseConfig", "KinematicModel", "Robot", "ToolConfig"]
