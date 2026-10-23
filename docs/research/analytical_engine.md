# AnalyticalEngine — аудит исходного кода

Дата исследования: 2026-10-23  
Репозиторий: https://github.com/CTD-Games/AnalyticalEngine  
Цель: понять, какие технические идеи пригодятся для отдельной HOI4-inspired игры на Rust. Это исследование структуры и выбранных исходников, не отчёт о сборке.

## Краткий вывод

**AnalyticalEngine не является готовым движком HOI4 и не является Rust/Android-проектом.** Это Java-проект, расширяющий Age of History 3 (AOC3): скриптовый слой, игровые события, условия и эффекты, моддинг, пользовательские интерфейсы, редакторы карты и сценариев. Для HOI4 Mobile он полезен прежде всего как архитектурный референс, а не как кодовая база для прямого переноса.

Рекомендация: изучать его модели и поведение, но проектировать собственные Rust-модули. Не считать данные и ресурсы свободными только потому, что в репозитории есть MIT LICENSE.

## Что подтверждено просмотром

### 1. Структура и технологии

- Корень содержит pom.xml, src/, docs/, META-INF/, target/, файлы IntelliJ и скрипты Windows.
- В pom.xml указаны зависимости libGDX gdx и gdx-backend-lwjgl3 версии 1.11.0. Maven-конфигурация указывает source/target Java 8 и одновременно --enable-preview; README отдельно требует Java SDK 23. Эти настройки и инструкции не полностью согласованы, поэтому реальную воспроизводимость сборки нужно проверять отдельно.
- В src/aoc/kingdoms/lukasz/jakowski/AA_Game.java используются ApplicationAdapter, Gdx и внутренний Renderer. Это подтверждает libGDX как основу игрового цикла/рендеринга в просмотренном коде.
- src/aoc/kingdoms/lukasz/jakowski/desktop/DesktopLauncher.java использует Lwjgl3Application и Lwjgl3ApplicationConfiguration: проверенный launcher рассчитан на desktop.
- В src/ есть нативные библиотеки libGDX для разных платформ, в том числе с суффиксами arm и arm64. Само наличие бинарников не доказывает наличие готовой Android-сборки; в просмотренной корневой структуре не обнаружен очевидный Android Gradle-модуль или Android launcher.
- Репозиторий содержит и .java, и .class. AA_Game.java и ProvinceDrawDetails.java помечены как код, восстановленный FernFlower-декомпилятором. Значит, часть исходников может быть декомпиляцией оригинальной игры, а не чистой авторской реализацией AnalyticalEngine.

### 2. Основной вход и интеграция с игрой

Файл [AnalyticalEngine.java](https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/AnalyticalEngine.java) показывает singleton-подобный объект AnalyticalEngine, статические ссылки на AA_Game и ScriptEngine, а также initialise(), который поднимает JavaScript через Javascript.initialiseJavascript() и вызывает initialiseGlobal().

Это означает, что AnalyticalEngine не запускает независимый симулятор с собственной моделью мира: он интегрируется с существующим AOC3-классом AA_Game и получает доступ к игровым объектам.

### 3. JavaScript как слой расширения

Файл [Javascript.java](https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/Framework/Javascript.java) содержит инфраструктуру запуска JS через ScriptEngine, обработку исключений и вспомогательные импорты Java-классов.

В src/AnalyticalEngine/GameFramework/startup_imports.js классы AOC3 импортируются через Java.type(...). global_initialisation.js создаёт общий namespace с main.events, main.gamestate, main.interfaces и коллекцией игровых scopes. startup.js последовательно инициализирует глобальное состояние, консоль, обработчики UI/карты, логику меню, сохранение/загрузку и затем моды.

**Архитектурный смысл:** скриптовый слой не заменяет игровой движок — он предоставляет надстройку и API к существующей Java-модели. Для Rust-проекта похожая идея полезна как data-driven слой сценариев, но API нужно проектировать поверх собственной модели мира, а не копировать связность AOC3.

### 4. Подсистемы, найденные в дереве исходников

| Путь | Наблюдаемая роль |
|---|---|
| src/AnalyticalEngine/Framework/ | Java/JavaScript bridge, клавиатура, общие методы, совместимость ОС и Steam |
| src/AnalyticalEngine/GameFramework/core/map/ | карта, регионы, ресурсы, terrain, сценарии и скриптовая работа с картой |
| src/AnalyticalEngine/GameFramework/core/provinces/ | операции над провинциями |
| src/AnalyticalEngine/GameFramework/core/military/ | функции для генералов, войск и военных действий |
| src/AnalyticalEngine/GameFramework/core/event_handler/ | обработчики игровых и UI-событий |
| src/AnalyticalEngine/GameFramework/core/ui/ | меню, страницы, подсказки, клавиатурные и мышиные события |
| src/AnalyticalEngine/GameFramework/core/mod/ | обнаружение и загрузка модов |
| src/AnalyticalEngine/Logic/Conditions/ | условия для глобального, провинциального и цивилизационного контекстов |
| src/AnalyticalEngine/Logic/Effects/ | эффекты для разных контекстов |
| src/AnalyticalEngine/Logic/Scopes/ | игровые scopes и on-action сценарии |
| src/AnalyticalEngine/Logic/Parsers/ | парсеры логики и переменных |
| src/AnalyticalEngine/JS/ | утилиты для массивов, дат, чисел, строк, поиска и A* pathfinding |
| src/AnalyticalEngine/Java/MapEditor/ | инструменты редактора карты и типов местности |
| src/aoc/kingdoms/lukasz/jakowski/ | игровые классы AOC3; часть исходников восстановлена из .class |
| src/aoc/kingdoms/lukasz/map/ | игровые менеджеры и объекты карты AOC3 |

Это перечень по фактическому дереву каталогов, а не утверждение, что каждая подсистема завершена или полностью протестирована.

### 5. Детали поведения из выбранных файлов

- [map_framework.js](https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/GameFramework/core/map/map_framework.js): clearMap() сбрасывает цвета провинций, а clearMapmode() возвращает стандартный terrain mapmode и обновляет отрисовку провинций.
- [province_framework.js](https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/GameFramework/core/provinces/province_framework.js): getAllProvinces() перебирает провинции через Game.getProvincesSize() и Game.getProvince(i), то есть использует существующий объектный API игры.
- [mod_framework.js](https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/GameFramework/core/mod/mod_framework.js): поиск каталогов модов завязан на SteamManager.modsFolders; это интеграция с текущей платформой/игрой, а не автономная переносимая система.
- [event_handler_framework.js](https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/GameFramework/core/event_handler/event_handler_framework.js): обработчики могут запускать периодические проверки через setInterval. Для мобильного симулятора нужно оценивать количество таймеров, частоту обновления и воспроизводимость.
- [pathfinding_a_star.js](https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/JS/pathfinding_a_star.js): есть A* по графу узлов с весами рёбер и восстановлением пути. Это концептуальный референс, но корректность и производительность на больших графах не бенчмаркировались.
- [deepscript_scope_parser.js](https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/Logic/Scopes/deepscript_scope_parser.js): API событий поддерживает условия, эффекты и варианты выбора; это полезный образец data-driven событий, но не готовая реализация HOI4 focus tree.

## Что полезно для HOI4 Mobile на Rust

### Идеи для адаптации

1. **Событийная модель.** Разделить триггеры, условия, эффекты и обработчики; хранить контент декларативно, исполнять через проверяемые Rust-функции.
2. **Scopes и контексты.** Явно определить World, Country, State, Province, Unit и Battle, чтобы игровые действия не зависели от глобальных неструктурированных объектов.
3. **Разделение данных карты и map mode.** Состояние провинций хранить отдельно от визуальной окраски и текущего режима карты; переключение map mode не должно менять симуляцию.
4. **Обработка пользовательских действий.** Выбор провинции/страны → игровая команда → проверка правил → изменение модели → обновление UI.
5. **Инструменты разработчика.** Консоль, диагностика событий, проверка данных и редактор сценариев полезны после базовой модели карты и симуляции.
6. **Поиск пути.** A* — только отправная точка. Для HOI4-подобной игры нужны собственные правила проходимости, местности, контроля территории, снабжения и переходов между сушей/морем/воздухом.

### Что не следует копировать как архитектурное решение

- Глобальные изменяемые JS-объекты main и global как основу симуляции: это осложняет типобезопасность, сериализацию, многопоточность и тестирование.
- Привязку логики к Java-классам AOC3, SteamManager и Game: Rust-проекту нужна собственная модель домена.
- Множество setInterval как главный планировщик стратегического времени. Лучше централизованный детерминированный тик/календарь и очередь запланированных событий.
- Считать AOC3 province map или игровые файлы источником карты HOI4: форматы, география, масштаб и правила разных игр не эквивалентны.
- Переносить декомпилированные классы, текстуры, карты, звуки или другие материалы оригинальной игры без отдельной проверки прав.

## Ограничения и риски

- README описывает AnalyticalEngine как beta и прямо отмечает частичную реализацию отдельных функций; поддержку модифицированного multiplayer — как тестируемую.
- README упоминает JDK 23, а pom.xml задаёт source/target 8 и preview-флаг. Нельзя делать вывод о воспроизводимой сборке без отдельного запуска в чистом окружении.
- Проверены структура каталогов и выбранные файлы, но **не выполнены** локальная компиляция, запуск, профилирование или полный построчный аудит всего большого репозитория.
- MIT LICENSE есть в репозитории, но это само по себе не доказывает, что все bundled assets, бинарники и декомпилированные элементы оригинальной AOC3 покрыты этой лицензией. Для нового проекта безопаснее реализовывать собственные данные и код, а заимствования ограничить идеями и интерфейсами после проверки лицензий.

## Следующий узкий этап исследования

Если продолжать изучение именно AnalyticalEngine, следующий этап — разобрать отрисовку карты и данные провинций: aoc/kingdoms/lukasz/map/map/Map.java, Map_Data.java, province/ProvinceDraw.java, Renderer/Renderer.java и связанные файлы GameFramework/core/map/. Нужно установить, как устроены геометрия провинций, выбор провинции касанием/мышью, кэширование, map modes и обновление цвета. Это следует использовать только как ориентир для собственной Rust-реализации.

## Основные ссылки

- [AnalyticalEngine.java](https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/AnalyticalEngine.java)
- [Javascript.java](https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/Framework/Javascript.java)
- [GameFramework](https://github.com/CTD-Games/AnalyticalEngine/tree/main/src/AnalyticalEngine/GameFramework)
- [Map framework](https://github.com/CTD-Games/AnalyticalEngine/tree/main/src/AnalyticalEngine/GameFramework/core/map)
- [Province framework](https://github.com/CTD-Games/AnalyticalEngine/tree/main/src/AnalyticalEngine/GameFramework/core/provinces)
- [Conditions](https://github.com/CTD-Games/AnalyticalEngine/tree/main/src/AnalyticalEngine/Logic/Conditions)
- [Effects](https://github.com/CTD-Games/AnalyticalEngine/tree/main/src/AnalyticalEngine/Logic/Effects)
- [pom.xml](https://github.com/CTD-Games/AnalyticalEngine/blob/main/pom.xml)
- [LICENSE](https://github.com/CTD-Games/AnalyticalEngine/blob/main/LICENSE)
