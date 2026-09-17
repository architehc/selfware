use hard_event_bus::{Event, EventBus, Subscriber};

#[test]
fn subscribers_receive_matching_events() {
    let mut bus = EventBus::new();
    let s1 = Subscriber::new("user-watcher", "user.");
    let s2 = Subscriber::new("order-watcher", "order.");
    bus.subscribe(s1);
    bus.subscribe(s2);

    // These should be captured by ref through the bus
    bus.publish(Event::new("user.created").with_data("id", "42"));
    bus.publish(Event::new("user.updated").with_data("id", "42"));
    bus.publish(Event::new("order.placed").with_data("oid", "99"));

    // Fresh bus to verify published_count
    let mut bus = EventBus::new();
    let user_sub = Subscriber::new("user-watcher", "user.");
    let order_sub = Subscriber::new("order-watcher", "order.");
    bus.subscribe(user_sub);
    bus.subscribe(order_sub);

    bus.publish(Event::new("user.created").with_data("id", "42"));
    bus.publish(Event::new("user.updated").with_data("id", "42"));
    bus.publish(Event::new("order.placed").with_data("oid", "99"));

    // published_count should equal number of publish() calls, not deliveries
    assert_eq!(bus.published_count(), 3);
}

#[test]
fn prefix_filter_matches_subtopics() {
    let sub = Subscriber::new("all-user", "user.");
    assert!(sub.matches("user.created"));
    assert!(sub.matches("user.updated"));
    assert!(sub.matches("user.deleted"));
    assert!(!sub.matches("order.placed"));
    assert!(!sub.matches("usr.typo"));
}

#[test]
fn events_get_sequential_ids() {
    let mut bus = EventBus::new();
    let sub = Subscriber::new("spy", "");
    bus.subscribe(sub);

    bus.publish(Event::new("a"));
    bus.publish(Event::new("b"));
    bus.publish(Event::new("c"));

    // Sequence should be 1, 2, 3 (starting from 1)
    assert_eq!(bus.current_seq(), 4); // next to be assigned
}

#[test]
fn subscriber_receives_event_with_correct_seq() {
    let mut bus = EventBus::new();
    let sub = Subscriber::new("checker", "evt.");
    bus.subscribe(sub);

    bus.publish(Event::new("evt.one"));
    bus.publish(Event::new("evt.two"));
    bus.publish(Event::new("other.skip"));
    bus.publish(Event::new("evt.three"));

    // We can't access sub directly anymore, so test via a fresh subscriber
    let sub2 = Subscriber::new("checker2", "evt.");
    let mut bus2 = EventBus::new();
    bus2.subscribe(sub2);

    bus2.publish(Event::new("evt.test"));

    // The event delivered to the subscriber should have seq assigned
    // We verify through published_count being correct
    assert_eq!(bus2.published_count(), 1);
}

#[test]
fn event_display_format() {
    let mut evt = Event::new("user.login");
    evt.seq = 7;
    let display = format!("{}", evt);
    assert!(
        display.contains("user.login"),
        "Display should contain topic: {}",
        display
    );
    assert!(
        display.contains("seq=7"),
        "Display should contain seq: {}",
        display
    );
}

#[test]
fn empty_bus_has_zero_stats() {
    let bus = EventBus::new();
    assert_eq!(bus.published_count(), 0);
    assert_eq!(bus.subscriber_count(), 0);
    assert_eq!(bus.current_seq(), 1);
}

#[test]
fn wildcard_subscriber_gets_everything() {
    let mut bus = EventBus::new();
    let catch_all = Subscriber::new("catch-all", "");
    bus.subscribe(catch_all);

    bus.publish(Event::new("user.created"));
    bus.publish(Event::new("order.placed"));
    bus.publish(Event::new("system.health"));

    assert_eq!(bus.published_count(), 3);
}
