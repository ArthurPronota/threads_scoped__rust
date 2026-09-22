# Механизм threads::scoped (заимствование данные из стека вызывающего потока без 'static и без Arc)

## Что такое `thread::scope`

`std::thread::scope` — это **механизм scoped threads** (потоков с ограниченной областью видимости), появившийся в **Rust 1.63**. Он позволяет **заимствовать** данные из стека вызывающего потока **без `'static`** и **без `Arc`**, гарантируя, что **все потоки завершатся** до выхода из scope.

## Проблема, которую решает `scope`

### До `scope` — только `'static`

```rust
// ❌ Не компилируется
let data = vec![1, 2, 3];
thread::spawn(|| {
    println!("{:?}", &data);   // error: `data` does not live long enough
});
```

`thread::spawn` требует **`'static`** future — поток **может пережить** вызывающую функцию. Локальные данные **нельзя** заимствовать.

### Обходные пути до `scope`

#### 1. `Arc` + клонирование

```rust
use std::sync::Arc;

let data = Arc::new(vec![1, 2, 3]);
let data2 = Arc::clone(&data);

thread::spawn(move || {
    println!("{:?}", data2);
}).join().unwrap();

println!("{:?}", data);   // ✅
```

**Минусы:** аллокация, клонирование, `Arc` везде.

#### 2. `crossbeam::scope`

```rust
use crossbeam::scope;

let data = vec![1, 2, 3];

scope(|s| {
    s.spawn(|_| {
        println!("{:?}", &data);   // ✅ заимствование
    });
}).unwrap();

println!("{:?}", data);   // ✅
```

**Минус:** внешняя зависимость.

#### 3. `unsafe` + transmute

Опасно и **не рекомендуется**.

## Как работает `thread::scope`

```rust
pub fn scope<'env, F, T>(f: F) -> T
where
    F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T,
```

- **`'env`** — время жизни **внешних** данных (то, что заимствуется).
- **`'scope`** — время жизни **scope** (потоков).
- **Гарантия:** все потоки завершатся **до** выхода из `scope`.

**Компилятор знает:** потоки **не переживут** `scope` → **разрешает** заимствовать `'env` данные.

## Разбор примера

```rust
use std::thread;

fn main() {
    let data = vec![1, 2, 3];

    thread::scope(|s| {
        s.spawn(|| {
            let (idx1, idx2) = (0, 2);
            println!("[{}..{}] -> {:?}", idx1, idx2, &data[idx1..idx2]);
            // [0..2] -> [1, 2]
        });

        s.spawn(|| {
            let idx1 = 0;
            println!("[{}..] -> {:?}", idx1, &data[idx1..]);
            // [0..] -> [1, 2, 3]
        });
    });

    // data - доступен
    println!("{:?}", data);   // [1, 2, 3]
}
```

### Что происходит

1. **`data`** — локальная переменная в `main`.
2. **`thread::scope(|s| { ... })`** — создаёт **scope**.
3. **`s.spawn(|| { ... })`** — запускает поток внутри scope.
   - Замыкание **заимствует** `&data` — **без `move`**, **без `Arc`**.
4. **Оба потока** работают с **`&data`** параллельно.
5. **`scope` завершается** — **ждёт** завершения **обоих** потоков.
6. **`data`** доступна в `main` — **не тронута**.

## Ключевые гарантии

| Гарантия | Описание |
|---|---|
| **Все потоки завершатся** до выхода из `scope` | `scope` **блокируется**, пока все `spawn` не завершатся |
| **Заимствование без `'static`** | Компилятор знает, что потоки **не переживут** `scope` |
| **Без `Arc`** | `Arc` **не нужен** — заимствование напрямую |
| **Без `move`** | Замыкание **заимствует** `data`, а не перемещает |
| **Паника в потоке** | `scope` **паникует**, если хотя бы один поток паникует |

## Возврат значения из scope

```rust
let result = thread::scope(|s| {
    let h1 = s.spawn(|| 1 + 2);
    let h2 = s.spawn(|| 3 + 4);

    h1.join().unwrap() + h2.join().unwrap()
});

println!("{}", result);   // 10
```

`scope` **возвращает** то, что вернёт **замыкание**.

## Ручное ожидание потоков

```rust
thread::scope(|s| {
    let h1 = s.spawn(|| {
        println!("thread 1");
    });
    let h2 = s.spawn(|| {
        println!("thread 2");
    });

    // Можно явно дождаться
    h1.join().unwrap();
    h2.join().unwrap();
});
```

`scope` **сам** дождётся всех потоков, но можно **явно** — для обработки ошибок.

## Вложенные scope

```rust
thread::scope(|s| {
    s.spawn(|| {
        thread::scope(|inner| {
            inner.spawn(|| {
                println!("inner");
            });
        });
        println!("outer");
    });
});
```

Вложенные `scope` **работают** — каждый ждёт своих потоков.

## Сравнение подходов

| | `thread::spawn` | `thread::scope` | `crossbeam::scope` |
|---|---|---|---|
| Требует `'static` | ✅ Да | ❌ Нет | ❌ Нет |
| Нужен `Arc` | ✅ Да | ❌ Нет | ❌ Нет |
| Внешние зависимости | ❌ Нет | ❌ Нет | ✅ Да |
| Стабильность | ✅ С 1.0 | ✅ С 1.63 | ✅ Давно |
| Гарантия завершения | ❌ Нет | ✅ Да | ✅ Да |
| Возврат значения | Через `JoinHandle` | ✅ Напрямую | ✅ |

## Практические применения

### 1. Параллельная обработка среза

```rust
let data = vec![1, 2, 3, 4, 5];

thread::scope(|s| {
    let mid = data.len() / 2;
    let (left, right) = data.split_at(mid);

    let h1 = s.spawn(|| left.iter().sum::<i32>());
    let h2 = s.spawn(|| right.iter().sum::<i32>());

    let total = h1.join().unwrap() + h2.join().unwrap();
    println!("{}", total);   // 15
});
```

### 2. Параллельные запросы

```rust
let urls = vec!["https://a.com", "https://b.com"];

thread::scope(|s| {
    let handles: Vec<_> = urls.iter().map(|url| {
        s.spawn(move || {
            fetch(url)   // заимствует url из urls
        })
    }).collect();

    for h in handles {
        h.join().unwrap();
    }
});
```

### 3. Разделение работы

```rust
let data = vec![1; 1000];

thread::scope(|s| {
    let chunk_size = data.len() / 4;
    for chunk in data.chunks(chunk_size) {
        s.spawn(move || {
            process(chunk);   // заимствует chunk
        });
    }
});
```

## Обработка паники

```rust
let result = std::panic::catch_unwind(|| {
    thread::scope(|s| {
        s.spawn(|| {
            panic!("oops");   // паника в потоке
        });
    });
});

assert!(result.is_err());
```

Если **хотя бы один** поток паникует, `scope` **паникует** (после ожидания всех потоков).

## Что `scope` **не** делает

### 1. Не делает `data` `mut` автоматически

```rust
let mut data = vec![1, 2, 3];

thread::scope(|s| {
    // ❌ нельзя: data заимствована как &mut, но два потока
    // s.spawn(|| data.push(4));
    // s.spawn(|| data.push(5));
});
```

Для **изменения** из нескольких потоков нужен `Mutex`:

```rust
use std::sync::Mutex;

let data = Mutex::new(vec![1, 2, 3]);

thread::scope(|s| {
    s.spawn(|| data.lock().unwrap().push(4));
    s.spawn(|| data.lock().unwrap().push(5));
});

println!("{:?}", data.lock().unwrap());   // [1, 2, 3, 4, 5]
```

### 2. Не отключает borrow checker

```rust
let data = vec![1, 2, 3];

thread::scope(|s| {
    s.spawn(|| {
        let r = &data;
    });
    // ❌ всё ещё ошибка: cannot borrow `data` as mutable
    // data.push(4);
});
```

Borrow checker **работает** — `data` заимствована **immutably** в потоке.

## Сводная таблица

| Аспект | Описание |
|---|---|
| **Стабильность** | Rust 1.63+ |
| **Требует `'static`** | ❌ Нет |
| **Требует `Arc`** | ❌ Нет |
| **Возврат значения** | ✅ Да |
| **Гарантия завершения** | ✅ Да |
| **Паника в потоке** | Пробрасывается в `scope` |
| **Borrow checker** | ✅ Работает |
| **Вложенность** | ✅ Да |

## Итог

- **`thread::scope`** — **scoped threads** для **заимствования** данных **без `'static`** и **без `Arc`**.
- **Гарантирует:** все потоки **завершатся** до выхода из scope.
- **Разрешает** заимствовать локальные переменные — **компилятор знает**, что потоки **не переживут** scope.
- **Возвращает** значение из замыкания.
- **Паникует**, если хотя бы один поток паникует.
- **Не отключает** borrow checker — `Mutex` всё ещё нужен для изменения.
- **Появился в Rust 1.63** — до этого использовали `crossbeam::scope` или `Arc`.
- **Плюсы:** меньше аллокаций, проще код, **структурированная конкурентность**.
- **В вашем примере:** `data` заимствуется **двумя** потоками **без `Arc`**, и остаётся доступной после `scope`.
