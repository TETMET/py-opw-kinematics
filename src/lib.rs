mod kinematic_model;
use crate::kinematic_model::KinematicModel;

use nalgebra::{Isometry3, Quaternion, Translation3, UnitQuaternion};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use polars::frame::DataFrame;
use polars::prelude::*;
use polars::series::Series;

use pyo3_polars::PyDataFrame;
use rs_opw_kinematics::kinematic_traits::{Kinematics, Pose, CONSTRAINT_CENTERED};
use rs_opw_kinematics::tool::{Base, Tool};

#[pyclass]
struct Robot {
    base_config: BaseConfig,
    tool_config: ToolConfig,
    _tool: Tool,
    _kinematic_model: KinematicModel,
}

#[pyclass]
#[derive(Clone, Debug)]
struct BaseConfig {
    /// The translation of the base in the world frame
    translation: [f64; 3],
    /// The rotation of the base in quaternion (w, x, y, z)
    rotation: [f64; 4],
}

#[pymethods]
impl BaseConfig {
    #[new]
    fn new(translation: [f64; 3], rotation: [f64; 4]) -> Self {
        BaseConfig {
            translation,
            rotation,
        }
    }
}

#[pyclass]
#[derive(Clone, Debug)]
struct ToolConfig {
    /// The translation of the tool in the base frame
    translation: [f64; 3],
    /// The rotation of the tool in quaternion (w, x, y, z)
    rotation: [f64; 4],
}

#[pymethods]
impl ToolConfig {
    #[new]
    fn new(translation: [f64; 3], rotation: [f64; 4]) -> Self {
        ToolConfig {
            translation,
            rotation,
        }
    }
}

#[pymethods]
impl Robot {
    #[new]
    #[pyo3(signature = (kinematic_model, base_config, tool_config))]
    fn new(
        kinematic_model: KinematicModel,
        base_config: BaseConfig,
        tool_config: ToolConfig,
    ) -> PyResult<Self> {
        let robot = kinematic_model.to_opw_kinematics();

        let base = Isometry3::from_parts(
            Translation3::from(base_config.translation),
            UnitQuaternion::from_quaternion(Quaternion::new(
                base_config.rotation[0],
                base_config.rotation[1],
                base_config.rotation[2],
                base_config.rotation[3],
            )),
        );

        let tool = Isometry3::from_parts(
            Translation3::from(tool_config.translation),
            UnitQuaternion::from_quaternion(Quaternion::new(
                tool_config.rotation[0],
                tool_config.rotation[1],
                tool_config.rotation[2],
                tool_config.rotation[3],
            )),
        );

        let robot_with_base = Base {
            robot: Arc::new(robot),
            base,
        };

        let robot_on_base_with_tool = Tool {
            robot: Arc::new(robot_with_base),
            tool,
        };

        // Create an instance with initial values
        let robot_instance = Robot {
            base_config,
            tool_config,
            _tool: robot_on_base_with_tool,
            _kinematic_model: kinematic_model,
        };

        Ok(robot_instance)
    }

    fn __repr__(&self) -> String {
        let km_repr = self
            ._kinematic_model
            .__repr__()
            .lines()
            .map(|line| format!("    {}", line)) // Indent each line of KinematicModel's repr with 4 spaces
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "Robot(\n    kinematic_model=\n{},\n    base_config={:?},\n    tool_config={:?}\n)",
            km_repr, self.base_config, self.tool_config
        )
    }

    /// Forward kinematics: calculates the pose for given joints in degrees
    fn forward(&self, joints: [f64; 6]) -> ([f64; 3], [f64; 4]) {
        let joints = joints.map(|x| x.to_radians());
        let pose: Pose = self._tool.forward(&joints);
        // Storage order is (x, y, z, w)
        let quat = [
            pose.rotation.coords[3],
            pose.rotation.coords[0],
            pose.rotation.coords[1],
            pose.rotation.coords[2],
        ];
        (pose.translation.vector.into(), quat)
    }

    fn convert_to_degrees(&self, joints: [f64; 6]) -> [f64; 6] {
        joints
            .iter()
            .map(|x| x.to_degrees())
            .collect::<Vec<f64>>()
            .try_into()
            .unwrap()
    }

    /// Calculates the axis configuration of the joints.
    ///
    /// # Arguments
    /// * `joints` - The joint values in degrees, as an array of 6 elements.
    ///
    /// # Returns
    /// * `[i32; 4]` - Returns the axis configuration, as an array of 4 elements.
    ///
    /// # Details
    /// The axis configuration is used to determine the robot's posture/quadrant, as defined in the ABB RAPID manual.
    /// For more information, see the RAPID manual: https://library.e.abb.com/public/e0de8b2e925a4ce486d8d95add172fff/3HAC050917%20TRM%20RAPID%20RW%206-en.pdf?x-sign=bIkaheN9PPYMPzuPk5eLTe%2fTo54jWW7tiYG10MDoKYaYzmGuvBwjMZnAe6RHfLoq
    #[pyo3(signature = (joints))]
    fn axis_configuration(&self, joints: [f64; 6]) -> [i32; 4] {
        let mut cfx = 0;
        if joints[4] < 0. {
            // Check if J5 is negative
            cfx += 1;
        }
        if joints[2] < -90. {
            // Check if J3 is negative
            cfx += 2;
        }
        if joints[1] < 0. {
            // Check if J2 is negative
            cfx += 4;
        }
        let cf1 = quadrant(joints[0]);
        let cf4 = quadrant(joints[3]);
        let cf6 = quadrant(joints[5]);
        [cf1, cf4, cf6, cfx]
    }

    /// Inverse kinematics: calculates the joint angles for a given pose.
    ///
    /// Uses the axis configuration to sort the solutions, prioritizing those that match the provided configuration.
    ///
    /// # Arguments
    /// * `pose` - The target pose as a tuple: ([x, y, z], [w, x, y, z]), where the translation is in meters and the rotation is a quaternion.
    /// * `current_joints` - (Optional) The current joint angles as an array of 6 elements (in degrees). Used as a seed for solution selection. If not provided, a default centered configuration is used.
    /// * `axis_configuration` - (Optional) The desired axis configuration as an array of 4 elements, used to sort and prioritize solutions. See ABB RAPID manual for details.
    ///
    /// # Returns
    /// * `Vec<[f64; 6]>` - A vector of possible joint solutions (in degrees), sorted by how well they match the axis configuration if provided.
    ///
    /// # Notes
    /// Singular solutions are filtered out. If `axis_configuration` is provided, solutions are sorted to prioritize matches.
    #[pyo3(signature = (pose, current_joints=None, axis_configuration=None))]
    fn inverse(
        &self,
        pose: ([f64; 3], [f64; 4]),
        current_joints: Option<[f64; 6]>,
        axis_configuration: Option<[i32; 4]>,
    ) -> Vec<[f64; 6]> {
        let quat = UnitQuaternion::from_quaternion(Quaternion::new(
            pose.1[0], pose.1[1], pose.1[2], pose.1[3],
        ));
        let iso_pose = Isometry3::from_parts(Translation3::from(pose.0), quat);

        let joints = if let Some(joints) = current_joints {
            joints.map(|x| x.to_radians())
        } else {
            CONSTRAINT_CENTERED
        };
        let solutions = self._tool.inverse_continuing(&iso_pose, &joints);

        // Calculate singularities and remove them from the solutions
        let singularities = solutions
            .iter()
            .map(|x| self._tool.kinematic_singularity(x))
            .collect::<Vec<_>>();

        let solutions = solutions
            .iter()
            .zip(singularities)
            .filter(|(_, singularity)| singularity.is_none())
            .map(|(x, _)| self.convert_to_degrees(*x))
            .collect::<Vec<_>>();

        if let Some(axis_configuration) = axis_configuration {
            let mut solutions_with_score: Vec<_> = solutions
                .iter()
                .map(|sol| {
                    let axis = self.axis_configuration(*sol);
                    // For now only sort by cfx because there seems to be something wrong with the other quadrants
                    let score = axis[3] == axis_configuration[3];
                    (score, *sol)
                })
                .collect();

            solutions_with_score.sort_by_key(|(score, _)| !*score);

            // Extract the sorted solutions
            let sorted_solutions: Vec<[f64; 6]> = solutions_with_score
                .into_iter()
                .map(|(_, sol)| sol)
                .collect();

            return sorted_solutions;
        }

        solutions
    }

    #[pyo3(signature = (poses, axis_configuration=None))]
    fn batch_inverse(
        &self,
        poses: PyDataFrame,
        axis_configuration: Option<[i32; 4]>,
    ) -> PyResult<PyDataFrame> {
        let df: DataFrame = poses.into();

        let x = extract_column_f64(&df, "X")?;
        let y = extract_column_f64(&df, "Y")?;
        let z = extract_column_f64(&df, "Z")?;
        let a = extract_column_f64(&df, "A")?;
        let b = extract_column_f64(&df, "B")?;
        let c = extract_column_f64(&df, "C")?;
        let d = extract_column_f64(&df, "D")?;

        // Use Vec<Option<f64>> to allow for None (Null) values
        let mut j1: Vec<Option<f64>> = Vec::with_capacity(df.height());
        let mut j2: Vec<Option<f64>> = Vec::with_capacity(df.height());
        let mut j3: Vec<Option<f64>> = Vec::with_capacity(df.height());
        let mut j4: Vec<Option<f64>> = Vec::with_capacity(df.height());
        let mut j5: Vec<Option<f64>> = Vec::with_capacity(df.height());
        let mut j6: Vec<Option<f64>> = Vec::with_capacity(df.height());

        for i in 0..df.height() {
            // Safely extract pose components, handling missing values
            let x_i = x.get(i);
            let y_i = y.get(i);
            let z_i = z.get(i);
            let a_i = a.get(i);
            let b_i = b.get(i);
            let c_i = c.get(i);
            let d_i = d.get(i);

            if let (Some(x), Some(y), Some(z), Some(a), Some(b), Some(c), Some(d)) =
                (x_i, y_i, z_i, a_i, b_i, c_i, d_i)
            {
                let pose = ([x, y, z], [a, b, c, d]);

                let solutions = self.inverse(pose, None, axis_configuration);
                if let Some(best_solution) = solutions.first() {
                    j1.push(Some(best_solution[0]));
                    j2.push(Some(best_solution[1]));
                    j3.push(Some(best_solution[2]));
                    j4.push(Some(best_solution[3]));
                    j5.push(Some(best_solution[4]));
                    j6.push(Some(best_solution[5]));
                } else {
                    // No solution found, push None values
                    j1.push(None);
                    j2.push(None);
                    j3.push(None);
                    j4.push(None);
                    j5.push(None);
                    j6.push(None);
                }
            } else {
                // Missing pose components, push None values
                j1.push(None);
                j2.push(None);
                j3.push(None);
                j4.push(None);
                j5.push(None);
                j6.push(None);
            }
        }

        // Create Series with optional values to allow Nulls
        let df_result = DataFrame::new(vec![
            Series::new("J1".into(), j1),
            Series::new("J2".into(), j2),
            Series::new("J3".into(), j3),
            Series::new("J4".into(), j4),
            Series::new("J5".into(), j5),
            Series::new("J6".into(), j6),
        ])
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;
        Ok(PyDataFrame(df_result))
    }

    #[pyo3(signature = (joints))]
    fn batch_forward(&self, joints: PyDataFrame) -> PyResult<PyDataFrame> {
        let df: DataFrame = joints.into();

        let j1 = extract_column_f64(&df, "J1")?;
        let j2 = extract_column_f64(&df, "J2")?;
        let j3 = extract_column_f64(&df, "J3")?;
        let j4 = extract_column_f64(&df, "J4")?;
        let j5 = extract_column_f64(&df, "J5")?;
        let j6 = extract_column_f64(&df, "J6")?;

        // Use Vec<Option<f64>> to allow for None (Null) values
        let mut x: Vec<Option<f64>> = Vec::with_capacity(df.height());
        let mut y: Vec<Option<f64>> = Vec::with_capacity(df.height());
        let mut z: Vec<Option<f64>> = Vec::with_capacity(df.height());
        let mut a: Vec<Option<f64>> = Vec::with_capacity(df.height());
        let mut b: Vec<Option<f64>> = Vec::with_capacity(df.height());
        let mut c: Vec<Option<f64>> = Vec::with_capacity(df.height());
        let mut d: Vec<Option<f64>> = Vec::with_capacity(df.height());

        for i in 0..df.height() {
            // Safely extract joint values, handling missing values
            let j1_i = j1.get(i);
            let j2_i = j2.get(i);
            let j3_i = j3.get(i);
            let j4_i = j4.get(i);
            let j5_i = j5.get(i);
            let j6_i = j6.get(i);

            if let (Some(j1), Some(j2), Some(j3), Some(j4), Some(j5), Some(j6)) =
                (j1_i, j2_i, j3_i, j4_i, j5_i, j6_i)
            {
                let joints_array = [j1, j2, j3, j4, j5, j6];
                let (translation, rotation) = self.forward(joints_array);

                x.push(Some(translation[0]));
                y.push(Some(translation[1]));
                z.push(Some(translation[2]));
                a.push(Some(rotation[0]));
                b.push(Some(rotation[1]));
                c.push(Some(rotation[2]));
                d.push(Some(rotation[3]));
            } else {
                // Missing joint values, push None values
                x.push(None);
                y.push(None);
                z.push(None);
                a.push(None);
                b.push(None);
                c.push(None);
                d.push(None);
            }
        }

        // Create Series with optional values to allow Nulls
        let df_result = DataFrame::new(vec![
            Series::new("X".into(), x),
            Series::new("Y".into(), y),
            Series::new("Z".into(), z),
            Series::new("A".into(), a),
            Series::new("B".into(), b),
            Series::new("C".into(), c),
            Series::new("D".into(), d),
        ])
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;
        Ok(PyDataFrame(df_result))
    }
}

/// Module initialization for Python
#[pymodule(name = "_internal")]
fn py_opw_kinematics(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<KinematicModel>()?;
    m.add_class::<Robot>()?;
    m.add_class::<BaseConfig>()?;
    m.add_class::<ToolConfig>()?;
    Ok(())
}

// Define a function that extracts a column, casting it to Float64Chunked.
fn extract_column_f64(df: &DataFrame, column_name: &str) -> PyResult<Float64Chunked> {
    let column = df.column(column_name).map_err(|e| {
        PyErr::new::<PyValueError, _>(format!("Error extracting column '{}': {}", column_name, e))
    })?;

    // Attempt to cast the column to Float64 data type.
    let casted_column = column.cast(&DataType::Float64).map_err(|e| {
        PyErr::new::<PyValueError, _>(format!(
            "Error casting column '{}' to f64: {}",
            column_name, e
        ))
    })?;

    // Convert the casted Series to Float64Chunked.
    let chunked = casted_column.f64().map_err(|e| {
        PyErr::new::<PyValueError, _>(format!(
            "Error converting column '{}' to Float64Chunked: {}",
            column_name, e
        ))
    })?;

    Ok(chunked.clone()) // Return an owned clone to satisfy ownership requirements.
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABB_1660: KinematicModel = KinematicModel {
        a1: 0.150,  // Distance from base to J1 axis
        a2: -0.110, // Distance from J1 to J2 axis (parallel offset)
        b: 0.0,     // Distance from J2 to J3 axis (perpendicular offset)
        c1: 0.4865, // Distance from base to J2 axis (height)
        c2: 0.700,  // Distance from J2 to J3 axis (upper arm length)
        c3: 0.678,  // Distance from J3 to J4 axis (forearm length)
        c4: 0.135,  // Distance from J4 to J6 axis (wrist length)
        offsets: [0.0, 0.0, -std::f64::consts::FRAC_PI_2, 0.0, 0.0, 0.0],
        sign_corrections: [1, 1, 1, 1, 1, 1],
    };

    #[test]
    fn test_simple_forward() {
        let kinematic_model = ABB_1660;
        let base_config = BaseConfig {
            translation: [0.0, 0.0, 2.3],
            rotation: [0.0, 1.0, 0.0, 0.0],
        };
        let tool_config = ToolConfig {
            translation: [0.0, 0.0, 0.095],
            rotation: [
                -0.00012991440873552217,
                -0.968154906938256,
                -0.0004965996111545046,
                0.2503407964804168,
            ],
        };
        let robot = Robot::new(kinematic_model, base_config, tool_config).unwrap();
        let joints = [-103.1, -85.03, 19.06, -70.19, -35.87, 185.01];
        let (translation, rotation) = robot.forward(joints);
        assert_eq!(
            translation,
            [0.2000017014027134, -0.30003856402112994, 0.8999972858765594]
        );
        assert_eq!(
            rotation,
            [
                0.8518484534487618,
                0.13765321623120808,
                -0.46476827163476586,
                -0.19848490647852607
            ]
        );
    }

    #[test]
    fn test_simple_inverse() {
        let kinematic_model = ABB_1660;
        let base_config = BaseConfig {
            translation: [0.0, 0.0, 2.3],
            rotation: [0.0, 1.0, 0.0, 0.0],
        };
        let tool_config = ToolConfig {
            translation: [0.0, 0.0, 0.095],
            rotation: [
                -0.00012991440873552217,
                -0.968154906938256,
                -0.0004965996111545046,
                0.2503407964804168,
            ],
        };
        let robot = Robot::new(kinematic_model, base_config, tool_config).unwrap();
        let pose = (
            [0.2000017014027134, -0.30003856402112994, 0.8999972858765594],
            [
                0.8518484534487618,
                0.13765321623120808,
                -0.46476827163476586,
                -0.19848490647852607,
            ],
        );
        let solutions = robot.inverse(pose, None, Some([0, 0, 0, 5]));
        assert_eq!(
            solutions,
            // Lots of solutions and not sure if all of them are great
            // Filtering should be done based on the quadrant and axis 6 normalization
            [
                // Axis configuration [0, 0, -1, 5]
                [
                    76.90000000000002,
                    -39.03534017507007,
                    32.97665093151276,
                    33.52534250992946,
                    -93.50495846663024,
                    -58.70431261343441
                ],
                // This is the ABB solution, right now it returns axis configuration [-2, -1, 2, 5]
                [
                    -103.09999999999998,
                    -85.03,
                    19.059999999999985,
                    -70.19000000000001,
                    -35.870000000000005,
                    -174.98999999999998
                ],
                [
                    -103.09999999999998,
                    -85.03,
                    19.059999999999985,
                    109.80999999999997,
                    35.870000000000005,
                    5.010000000000008
                ],
                [
                    76.90000000000002,
                    73.06238461570422,
                    165.4543038481549,
                    -61.28358770585671,
                    38.94565680362163,
                    -6.185291336143623
                ],
                [
                    76.90000000000002,
                    -39.03534017507007,
                    32.97665093151276,
                    -146.47465749007054,
                    93.50495846663024,
                    121.29568738656559
                ],
                [
                    -103.09999999999998,
                    13.524762945285602,
                    179.37095477966767,
                    33.482250880445946,
                    87.80184590248223,
                    117.5230126608287
                ],
                [
                    -103.09999999999998,
                    13.524762945285602,
                    179.37095477966767,
                    -146.51774911955405,
                    -87.80184590248223,
                    -62.4769873391713
                ],
                [
                    76.90000000000002,
                    73.06238461570422,
                    165.4543038481549,
                    118.7164122941433,
                    -38.94565680362163,
                    173.81470866385638
                ]
            ]
        );
    }

    #[test]
    fn test_axis_configuration_cfx_5() {
        let kinematic_model = ABB_1660;
        let base_config = BaseConfig {
            translation: [0.0, 0.0, 2.3],
            rotation: [0.0, 1.0, 0.0, 0.0],
        };
        let tool_config = ToolConfig {
            translation: [0.0, 0.0, 0.095],
            rotation: [
                -0.00012991440873552217,
                -0.968154906938256,
                -0.0004965996111545046,
                0.2503407964804168,
            ],
        };
        let robot = Robot::new(kinematic_model, base_config, tool_config).unwrap();
        let joints = [-103.1, -85.03, 19.06, -70.19, -35.87, 185.01];
        let axis_configuration = robot.axis_configuration(joints);
        assert_eq!(axis_configuration, [-2, -1, 2, 5]);
    }

    #[test]
    fn test_axis_configuration_cfx_4() {
        let kinematic_model = ABB_1660;
        let base_config = BaseConfig {
            translation: [0.0, 0.0, 2.3],
            rotation: [0.0, 1.0, 0.0, 0.0],
        };
        let tool_config = ToolConfig {
            translation: [0.0, 0.0, 0.095],
            rotation: [
                -0.00012991440873552217,
                -0.968154906938256,
                -0.0004965996111545046,
                0.2503407964804168,
            ],
        };
        let robot = Robot::new(kinematic_model, base_config, tool_config).unwrap();
        let joints = [-133.69, -57.37, -33.13, -78.0, 54.53, -66.13];
        let axis_configuration = robot.axis_configuration(joints);
        assert_eq!(axis_configuration, [-2, -1, -1, 4]);
    }
}

fn quadrant(joint: f64) -> i32 {
    (joint / 90.0).floor() as i32
}
