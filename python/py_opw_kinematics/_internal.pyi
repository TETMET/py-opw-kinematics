from typing import List, Tuple, Optional

class KinematicModel:
    a1: float
    a2: float
    b: float
    c1: float
    c2: float
    c3: float
    c4: float
    offsets: Tuple[float, float, float, float, float, float]
    flip_axes: Optional[Tuple[bool, bool, bool, bool, bool, bool]]
    has_parallelogram: bool

    def __init__(
        self,
        a1: float = 0,
        a2: float = 0,
        b: float = 0,
        c1: float = 0,
        c2: float = 0,
        c3: float = 0,
        c4: float = 0,
        offsets: Tuple[float, float, float, float, float, float] = (
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        ),
        flip_axes: Optional[Tuple[bool, bool, bool, bool, bool, bool]] = (
            False,
            False,
            False,
            False,
            False,
            False,
        ),
        has_parallelogram: bool = False,
    ) -> None:
        """
        Initializes a KinematicModel instance.

        :param a1, a2, b, c1, c2, c3, c4: Kinematic parameters.
        :param offsets: Joint offsets.
        :param flip_axes: Boolean flags for flipping axes.
        :param has_parallelogram: Indicates if the model has a parallelogram linkage.
        """
        ...

class BaseConfig:
    translation: Tuple[float, float, float]
    rotation: Tuple[float, float, float, float]

    def __init__(
        self,
        translation: Tuple[float, float, float],
        rotation: Tuple[float, float, float, float],
    ) -> None: ...

class ToolConfig:
    translation: Tuple[float, float, float]
    rotation: Tuple[float, float, float, float]

    def __init__(
        self,
        translation: Tuple[float, float, float],
        rotation: Tuple[float, float, float, float],
    ) -> None: ...

class Robot:
    def __init__(
        self,
        kinematic_model: KinematicModel,
        base_config: BaseConfig,
        tool_config: ToolConfig,
    ) -> None:
        """
        Initializes a Robot instance.

        :param kinematic_model: The kinematic model of the robot.
        :param base_config: The base configuration of the robot.
        :param tool_config: The tool configuration of the robot.
        """
        ...

    def forward(
        self, joints: Tuple[float, float, float, float, float, float]
    ) -> Tuple[Tuple[float, float, float], Tuple[float, float, float, float]]:
        """
        Computes the forward kinematics for the given joint angles.

        :param joints: Joint angles of the robot in radians.
        :return: A tuple containing the position and quaternion of the tool in the world frame.
        """
        ...

    def inverse(
        self,
        pose: Tuple[Tuple[float, float, float], Tuple[float, float, float, float]],
        current_joints: Optional[
            Tuple[float, float, float, float, float, float]
        ] = None,
    ) -> List[Tuple[float, float, float, float, float, float]]:
        """
        Computes the inverse kinematics for a given pose.

        :param pose: Desired pose (position and quaternion) of the tool in the world frame.
        :param current_joints: Current joint configuration (optional).
        :return: A list of possible joint configurations that achieve the desired pose.
        """
        ...

    def batch_inverse(
        self,
        poses: List[
            Tuple[Tuple[float, float, float], Tuple[float, float, float, float]]
        ],
    ) -> List[List[Tuple[float, float, float, float, float, float]]]:
        """
        Computes the inverse kinematics for multiple poses in batch mode.

        :param poses: List of poses, each containing position and quaternion tuples.
        :return: List of lists containing all possible joint configurations for each pose.
        """
        ...

    def batch_forward(
        self, joints: List[Tuple[float, float, float, float, float, float]]
    ) -> List[Tuple[Tuple[float, float, float], Tuple[float, float, float, float]]]:
        """
        Computes the forward kinematics for multiple sets of joint angles in batch mode.

        :param joints: List of joint configurations (6 joint angles each).
        :return: List of poses, each containing position and quaternion tuples.
        """
        ...

__all__: List[str] = ["BaseConfig", "KinematicModel", "Robot", "ToolConfig"]
