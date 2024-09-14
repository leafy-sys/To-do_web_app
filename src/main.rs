// src/main.rs

#[macro_use]
extern crate rocket;

use dotenv::dotenv;
use mysql::prelude::*;
use mysql::Opts;
use mysql::*;
use rocket::response::status;
use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::State;
use std::env;
use std::sync::Mutex;

// Task struct for serialization/deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Task {
    id: Option<u32>,
    description: String,
    is_completed: bool,
}

// Database connection pool wrapped in a Mutex for thread safety
struct DbConnPool {
    pool: Mutex<Pool>,
}

// Function to create a new database pool
fn init_pool() -> Pool {
    // Load environment variables from .env file
    dotenv().ok();

    // Get the DATABASE_URL environment variable
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // Create Opts from the database URL
    let opts = Opts::from_url(&database_url).expect("Invalid database URL");

    // Create a new connection pool
    Pool::new(opts).expect("Failed to create database pool")
}

// Rocket routes

// List all tasks
#[get("/tasks")]
async fn list_tasks(db: &State<DbConnPool>) -> Json<Vec<Task>> {
    let pool = db.pool.lock().unwrap();
    let mut conn = pool.get_conn().unwrap();

    let tasks = conn
        .query_map(
            "SELECT id, description, is_completed FROM tasks",
            |(id, description, is_completed)| Task {
                id: Some(id),
                description,
                is_completed,
            },
        )
        .unwrap();

    Json(tasks)
}

// Get a single task by ID
#[get("/tasks/<task_id>")]
async fn get_task(db: &State<DbConnPool>, task_id: u32) -> Option<Json<Task>> {
    let pool = db.pool.lock().unwrap();
    let mut conn = pool.get_conn().unwrap();

    let result: Option<Task> = conn
        .exec_first(
            "SELECT id, description, is_completed FROM tasks WHERE id = :id",
            params! {
                "id" => task_id,
            },
        )
        .unwrap()
        .map(|(id, description, is_completed)| Task {
            id: Some(id),
            description,
            is_completed,
        });

    result.map(Json)
}

// Create a new task
#[post("/tasks", format = "json", data = "<task>")]
async fn create_task(db: &State<DbConnPool>, task: Json<Task>) -> status::Created<Json<Task>> {
    let pool = db.pool.lock().unwrap();
    let mut conn = pool.get_conn().unwrap();

    conn.exec_drop(
        "INSERT INTO tasks (description, is_completed) VALUES (:description, :is_completed)",
        params! {
            "description" => &task.description,
            "is_completed" => task.is_completed,
        },
    )
    .unwrap();

    let last_id = conn.last_insert_id() as u32;

    let new_task = Task {
        id: Some(last_id),
        description: task.description.clone(),
        is_completed: task.is_completed,
    };

    status::Created::new(format!("/tasks/{}", last_id)).body(Json(new_task))
}

// Update an existing task
#[put("/tasks/<task_id>", format = "json", data = "<task>")]
async fn update_task(db: &State<DbConnPool>, task_id: u32, task: Json<Task>) -> Option<Json<Task>> {
    let pool = db.pool.lock().unwrap();
    let mut conn = pool.get_conn().unwrap();

    let result = conn.exec_drop(
        "UPDATE tasks SET description = :description, is_completed = :is_completed WHERE id = :id",
        params! {
            "id" => task_id,
            "description" => &task.description,
            "is_completed" => task.is_completed,
        },
    );

    match result {
        Ok(_) => Some(Json(Task {
            id: Some(task_id),
            description: task.description.clone(),
            is_completed: task.is_completed,
        })),
        Err(_) => None,
    }
}

// Delete a task
#[delete("/tasks/<task_id>")]
async fn delete_task(db: &State<DbConnPool>, task_id: u32) -> status::NoContent {
    let pool = db.pool.lock().unwrap();
    let mut conn = pool.get_conn().unwrap();

    conn.exec_drop(
        "DELETE FROM tasks WHERE id = :id",
        params! {
            "id" => task_id,
        },
    )
    .unwrap();

    status::NoContent
}

// Ensure the tasks table exists
fn init_db() {
    let pool = init_pool();
    let mut conn = pool.get_conn().unwrap();

    conn.query_drop(
        r"CREATE TABLE IF NOT EXISTS tasks (
            id INT PRIMARY KEY AUTO_INCREMENT,
            description TEXT NOT NULL,
            is_completed BOOLEAN NOT NULL DEFAULT false
        )",
    )
    .unwrap();
}

// Launch the Rocket application
#[launch]
fn rocket() -> _ {
    init_db();
    let db_pool = DbConnPool {
        pool: Mutex::new(init_pool()),
    };

    rocket::build().manage(db_pool).mount(
        "/",
        routes![list_tasks, get_task, create_task, update_task, delete_task],
    )
}
