use crate::graphics::{Color, DrawTarget};
use crate::math::FloatExt;
use alloc::vec::Vec;
use core::ops::{Add, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3 { x, y, z }
    }

    pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn normalize(&mut self) {
        let mag = self.magnitude();
        if mag > 0.0001 {
            // Порог для избежания деления на 0
            self.x /= mag;
            self.y /= mag;
            self.z /= mag;
        }
    }

    pub fn normalized(&self) -> Vec3 {
        let mut v = *self;
        v.normalize();
        v
    }

    pub fn dot(&self, other: &Vec3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(&self, other: &Vec3) -> Vec3 {
        Vec3 {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }
}

// Реализуем арифметические операции для Vec3
impl Add for Vec3 {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Vec3 {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Vec3 {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, scalar: f32) -> Self::Output {
        Vec3 {
            x: self.x * scalar,
            y: self.y * scalar,
            z: self.z * scalar,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Mat4x4 {
    pub elements: [[f32; 4]; 4],
}

impl Mat4x4 {
    pub fn identity() -> Self {
        Mat4x4 {
            elements: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn translate(x: f32, y: f32, z: f32) -> Self {
        let mut m = Self::identity();
        m.elements[3][0] = x;
        m.elements[3][1] = y;
        m.elements[3][2] = z;
        m
    }

    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        let mut m = Self::identity();
        m.elements[0][0] = x;
        m.elements[1][1] = y;
        m.elements[2][2] = z;
        m
    }

    pub fn rotate_x(angle: f32) -> Self {
        let mut m = Self::identity();
        let cos_val = angle.cos();
        let sin_val = angle.sin();
        m.elements[1][1] = cos_val;
        m.elements[1][2] = sin_val;
        m.elements[2][1] = -sin_val;
        m.elements[2][2] = cos_val;
        m
    }

    pub fn rotate_y(angle: f32) -> Self {
        let mut m = Self::identity();
        let cos_val = angle.cos();
        let sin_val = angle.sin();
        m.elements[0][0] = cos_val;
        m.elements[0][2] = -sin_val;
        m.elements[2][0] = sin_val;
        m.elements[2][2] = cos_val;
        m
    }

    pub fn rotate_z(angle: f32) -> Self {
        let mut m = Self::identity();
        let cos_val = angle.cos();
        let sin_val = angle.sin();
        m.elements[0][0] = cos_val;
        m.elements[0][1] = sin_val;
        m.elements[1][0] = -sin_val;
        m.elements[1][1] = cos_val;
        m
    }

    pub fn multiply(&self, other: &Self) -> Self {
        let mut result = Self::identity();
        for i in 0..4 {
            for j in 0..4 {
                result.elements[i][j] = 0.0;
                for k in 0..4 {
                    result.elements[i][j] += self.elements[i][k] * other.elements[k][j];
                }
            }
        }
        result
    }

    pub fn transform_point(&self, point: &Vec3) -> Vec3 {
        let x = point.x * self.elements[0][0]
            + point.y * self.elements[1][0]
            + point.z * self.elements[2][0]
            + self.elements[3][0];
        let y = point.x * self.elements[0][1]
            + point.y * self.elements[1][1]
            + point.z * self.elements[2][1]
            + self.elements[3][1];
        let z = point.x * self.elements[0][2]
            + point.y * self.elements[1][2]
            + point.z * self.elements[2][2]
            + self.elements[3][2];
        let w = point.x * self.elements[0][3]
            + point.y * self.elements[1][3]
            + point.z * self.elements[2][3]
            + self.elements[3][3];

        if w != 0.0 {
            Vec3 {
                x: x / w,
                y: y / w,
                z: z / w,
            }
        } else {
            Vec3 { x, y, z }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Triangle3D {
    pub vertices: [Vec3; 3],
    pub color: Color,
}

impl Triangle3D {
    pub fn new(v1: Vec3, v2: Vec3, v3: Vec3, color: Color) -> Self {
        Triangle3D {
            vertices: [v1, v2, v3],
            color,
        }
    }

    pub fn normal(&self) -> Vec3 {
        let edge1 = Vec3 {
            x: self.vertices[1].x - self.vertices[0].x,
            y: self.vertices[1].y - self.vertices[0].y,
            z: self.vertices[1].z - self.vertices[0].z,
        };
        let edge2 = Vec3 {
            x: self.vertices[2].x - self.vertices[0].x,
            y: self.vertices[2].y - self.vertices[0].y,
            z: self.vertices[2].z - self.vertices[0].z,
        };
        edge1.cross(&edge2).normalized()
    }
}

pub struct Camera3D {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov: f32,    // угол обзора
    pub aspect: f32, // соотношение сторон
    pub near: f32,   // ближняя плоскость
    pub far: f32,    // дальняя плоскость
}

impl Camera3D {
    pub fn new(
        position: Vec3,
        target: Vec3,
        up: Vec3,
        fov: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> Self {
        Camera3D {
            position,
            target,
            up,
            fov,
            aspect,
            near,
            far,
        }
    }

    pub fn get_view_matrix(&self) -> Mat4x4 {
        let forward = (self.target - self.position).normalized();
        let right = forward.cross(&self.up.normalized());
        let up = right.cross(&forward);

        let mut view_matrix = Mat4x4::identity();
        view_matrix.elements[0][0] = right.x;
        view_matrix.elements[1][0] = right.y;
        view_matrix.elements[2][0] = right.z;

        view_matrix.elements[0][1] = up.x;
        view_matrix.elements[1][1] = up.y;
        view_matrix.elements[2][1] = up.z;

        view_matrix.elements[0][2] = -forward.x;
        view_matrix.elements[1][2] = -forward.y;
        view_matrix.elements[2][2] = -forward.z;

        // Позиция камеры
        let neg_camera_pos = Vec3 {
            x: -self.position.x,
            y: -self.position.y,
            z: -self.position.z,
        };

        view_matrix.elements[3][0] = right.dot(&neg_camera_pos);
        view_matrix.elements[3][1] = up.dot(&neg_camera_pos);
        view_matrix.elements[3][2] = forward.dot(&neg_camera_pos); // Поменял знак на +

        view_matrix
    }

    pub fn get_projection_matrix(&self) -> Mat4x4 {
        let f = 1.0 / (self.fov / 2.0).tan();

        Mat4x4 {
            elements: [
                [f / self.aspect, 0.0, 0.0, 0.0],
                [0.0, f, 0.0, 0.0],
                [
                    0.0,
                    0.0,
                    (self.far + self.near) / (self.near - self.far),
                    (2.0 * self.far * self.near) / (self.near - self.far),
                ],
                [0.0, 0.0, -1.0, 0.0],
            ],
        }
    }
}

// Рендерер 3D сцены
pub struct Scene3D {
    pub triangles: Vec<Triangle3D>,
    pub camera: Camera3D,
    pub transformation: Mat4x4,
    pub use_backface_culling: bool,
}

impl Scene3D {
    pub fn new(camera: Camera3D) -> Self {
        Scene3D {
            triangles: Vec::new(),
            camera,
            transformation: Mat4x4::identity(),
            use_backface_culling: true,
        }
    }

    pub fn add_triangle(&mut self, triangle: Triangle3D) {
        self.triangles.push(triangle);
    }

    pub fn clear_triangles(&mut self) {
        self.triangles.clear();
    }

    pub fn render(&self, target: &mut dyn DrawTarget) {
        let view_matrix = self.camera.get_view_matrix();
        let proj_matrix = self.camera.get_projection_matrix();

        let transform_view = self.transformation.multiply(&view_matrix);
        let mvp = transform_view.multiply(&proj_matrix);

        for triangle in &self.triangles {
            // Преобразуем вершины
            let mut transformed_vertices = [
                mvp.transform_point(&triangle.vertices[0]),
                mvp.transform_point(&triangle.vertices[1]),
                mvp.transform_point(&triangle.vertices[2]),
            ];

            // Проверяем, находится ли треугольник за камерой
            if transformed_vertices[0].z < 0.0
                && transformed_vertices[1].z < 0.0
                && transformed_vertices[2].z < 0.0
            {
                continue;
            }

            // Проверяем, нужен ли backface culling
            if self.use_backface_culling {
                let normal = triangle.normal();
                let eye_direction = Vec3 {
                    x: self.camera.position.x - triangle.vertices[0].x,
                    y: self.camera.position.y - triangle.vertices[0].y,
                    z: self.camera.position.z - triangle.vertices[0].z,
                };

                // Если скалярное произведение нормали и направления взгляда положительно,
                // то это задняя сторона - пропускаем
                if normal.dot(&eye_direction) > 0.0 {
                    continue;
                }
            }

            // Перспективная проекция
            for vertex in &mut transformed_vertices {
                if vertex.z != 0.0 {
                    vertex.x /= vertex.z;
                    vertex.y /= vertex.z;
                }
            }

            // Преобразуем в экранные координаты
            let width = target.width() as f32;
            let height = target.height() as f32;

            let screen_vertices = [
                (
                    ((transformed_vertices[0].x + 1.0) * width / 2.0) as usize,
                    ((-transformed_vertices[0].y + 1.0) * height / 2.0) as usize,
                ),
                (
                    ((transformed_vertices[1].x + 1.0) * width / 2.0) as usize,
                    ((-transformed_vertices[1].y + 1.0) * height / 2.0) as usize,
                ),
                (
                    ((transformed_vertices[2].x + 1.0) * width / 2.0) as usize,
                    ((-transformed_vertices[2].y + 1.0) * height / 2.0) as usize,
                ),
            ];

            // Ограничиваем координаты диапазоном экрана
            if screen_vertices
                .iter()
                .all(|&(x, y)| x < target.width() && y < target.height())
            {
                // Рисуем треугольник (временно рисуем только линии)
                self.draw_wireframe_triangle(target, screen_vertices, triangle.color);
            }
        }
    }

    fn draw_wireframe_triangle(
        &self,
        target: &mut dyn DrawTarget,
        vertices: [(usize, usize); 3],
        color: Color,
    ) {
        // Рисуем линии между вершинами
        self.draw_line(
            target,
            vertices[0].0,
            vertices[0].1,
            vertices[1].0,
            vertices[1].1,
            color,
        );
        self.draw_line(
            target,
            vertices[1].0,
            vertices[1].1,
            vertices[2].0,
            vertices[2].1,
            color,
        );
        self.draw_line(
            target,
            vertices[2].0,
            vertices[2].1,
            vertices[0].0,
            vertices[0].1,
            color,
        );
    }

    // Простая линия - использовать алгоритм Брезенхэма в реальной реализации
    fn draw_line(
        &self,
        target: &mut dyn DrawTarget,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        color: Color,
    ) {
        // Упрощенная реализация: просто рисуем точки
        let dx = (x1 as i32 - x0 as i32).abs();
        let dy = (y1 as i32 - y0 as i32).abs();

        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };

        let mut err = dx - dy;

        let mut x = x0 as i32;
        let mut y = y0 as i32;

        loop {
            if x >= 0 && y >= 0 && x < target.width() as i32 && y < target.height() as i32 {
                target.draw_pixel(x as usize, y as usize, color);
            }

            if x == x1 as i32 && y == y1 as i32 {
                break;
            }

            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn rotate_scene(&mut self, rx: f32, ry: f32, rz: f32) {
        let rot_x = Mat4x4::rotate_x(rx);
        let rot_y = Mat4x4::rotate_y(ry);
        let rot_z = Mat4x4::rotate_z(rz);

        let rotation = rot_x.multiply(&rot_y).multiply(&rot_z);
        self.transformation = self.transformation.multiply(&rotation);
    }

    pub fn translate_scene(&mut self, x: f32, y: f32, z: f32) {
        let translation = Mat4x4::translate(x, y, z);
        self.transformation = self.transformation.multiply(&translation);
    }

    pub fn scale_scene(&mut self, x: f32, y: f32, z: f32) {
        let scaling = Mat4x4::scale(x, y, z);
        self.transformation = self.transformation.multiply(&scaling);
    }
}

// Пример создания объекта - куба
pub fn create_cube(center: Vec3, size: f32, color: Color) -> Vec<Triangle3D> {
    let half_size = size / 2.0;
    let mut triangles = Vec::new();

    // Определяем вершины куба
    let vertices = [
        Vec3::new(
            center.x - half_size,
            center.y - half_size,
            center.z - half_size,
        ), // 0
        Vec3::new(
            center.x + half_size,
            center.y - half_size,
            center.z - half_size,
        ), // 1
        Vec3::new(
            center.x + half_size,
            center.y + half_size,
            center.z - half_size,
        ), // 2
        Vec3::new(
            center.x - half_size,
            center.y + half_size,
            center.z - half_size,
        ), // 3
        Vec3::new(
            center.x - half_size,
            center.y - half_size,
            center.z + half_size,
        ), // 4
        Vec3::new(
            center.x + half_size,
            center.y - half_size,
            center.z + half_size,
        ), // 5
        Vec3::new(
            center.x + half_size,
            center.y + half_size,
            center.z + half_size,
        ), // 6
        Vec3::new(
            center.x - half_size,
            center.y + half_size,
            center.z + half_size,
        ), // 7
    ];

    // Грани куба (каждая грань состоит из 2 треугольников)
    let indices = [
        // Передняя грань (z = -half_size)
        [0, 1, 2],
        [0, 2, 3],
        // Задняя грань (z = +half_size)
        [5, 4, 7],
        [5, 7, 6],
        // Левая грань (x = -half_size)
        [4, 0, 3],
        [4, 3, 7],
        // Правая грань (x = +half_size)
        [1, 5, 6],
        [1, 6, 2],
        // Нижняя грань (y = -half_size)
        [4, 5, 1],
        [4, 1, 0],
        // Верхняя грань (y = +half_size)
        [3, 2, 6],
        [3, 6, 7],
    ];

    for face in &indices {
        triangles.push(Triangle3D::new(
            vertices[face[0]],
            vertices[face[1]],
            vertices[face[2]],
            color,
        ));
    }

    triangles
}
