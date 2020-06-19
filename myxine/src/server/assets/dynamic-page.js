window.addEventListener("load", () => {

    // Print debug info if the user sets window.myxine = true
    window.myxine = { "debug": false };
    function debug(...args) {
        if (window.myxine.debug === true) {
            console.log(...args);
        }
    }

    // The initial set of listeners is empty
    let listeners = {};

    // Current animation frame callback ID, if any
    let animationId = null;

    // NOTE: Why do we use separate workers for events and query results, but
    // only one worker for all events? Well, it matters that events arrive in
    // order so using one worker forces them to be linearized. And having a
    // second, separate worker for query results means that query results can be
    // processed concurrently with events, since their interleaving doesn't
    // matter.

    // Actually send an event back to the server
    let sendEventWorker = new window.Worker("/.myxine/assets/post.js");

    function sendEvent(type, path, properties) {
        let url = window.location.href + "?page-event";
        let data = JSON.stringify({
            event: type,
            targets: path,
            properties: properties
        });
        debug("Sending event:", data);
        sendEventWorker.postMessage({
            url: url,
            contentType: "application/json",
            data: data
        });
    }

    // Actually send a query result back to the server
    let sendEvalResultWorker = new window.Worker("/.myxine/assets/post.js");

    function sendEvalResult(id, result) {
        let url = window.location.href
            + "?page-result="  + encodeURIComponent(id);
        sendEvalResultWorker.postMessage({
            url: url,
            contentType: "application/json",
            data: JSON.stringify(result),
        });
    }

    function sendEvalError(id, error) {
        let url = window.location.href
            + "?page-error="  + encodeURIComponent(id);
        sendEvalResultWorker.postMessage({
            url: url,
            contentType: "text/plain",
            data: error.toString()
        });
    }

    // Set the body
    function setBodyTo(body) {
        // Cancel the previous animation frame, if any
        if (animationId !== null) {
            window.cancelAnimationFrame(animationId);
        }
        // Redraw the body before the next repaint (but not right now yet)
        animationId = window.requestAnimationFrame(timestamp => {
            window.diff.innerHTML(document.body, body);
        });
    }

    // Evaluate a JavaScript expression in the global environment
    function evaluate(expression, statementMode) {
        // TODO: LRU-limited memoization of functions themselves (not their
        // results), possibly using memoizee?
        if (!statementMode) {
            return Function("return (" + expression + ")")();
        } else {
            return Function(expression)();
        }
    }

    // Evaluate a JavaScript expression and return the result
    function evaluateAndRespond(statementMode, event) {
        debug("Evaluating expression '" + event.data + "'(id "
              + event.lastEventId + ") as a"
              + (statementMode ? " statement" : "n expression"));
        try {
            let result = evaluate(event.data, statementMode);
            if (typeof result === "undefined") {
                result = null;
            }
            debug("Sending back result response (id "
                  + event.lastEventId
                  + "):", result);
            sendEvalResult(event.lastEventId, result);
        } catch(err) {
            debug("Sending back error response (id "
                  + event.lastEventId
                  + "):", err);
            sendEvalError(event.lastEventId, err);
        }
    }

    // Functions from JavaScript objects to serializable objects, keyed by the
    // types of those objects as represented in the interface description
    const customJsonFormatters = {
        // Add here if there's a need to support more complex object types
    };

    // Parse a description of events and interfaces, and return a mapping from
    // event name to mappings from property name -> formatter for that property
    function parseEventDescriptions(enabledEvents) {
        let events = {};
        const allEvents = enabledEvents.events;
        Object.entries(allEvents).forEach(([eventName, eventInfo]) => {
            // Accumulate the desired fields for the event into a map from
            // field name to formatter for the objects in that field
            let interfaceName = eventInfo["interface"]; // most specific
            let theInterface = enabledEvents.interfaces[interfaceName];
            events[eventName] = {};
            while (true) {
                const properties = Object.keys(theInterface.properties);
                properties.forEach(property => {
                    let formatter = customJsonFormatters[property];
                    if (typeof formatter === "undefined") {
                        formatter = (x => x); // Default formatter is id
                    }
                    if (typeof events[eventName][property] === "undefined") {
                        events[eventName][property] = formatter;
                    } else {
                        debug("Duplicate property in "
                              + eventName
                              + ": "
                              + property);
                    }
                });
                if (theInterface.inherits !== null) {
                    // Check ancestors for more fields to add
                    theInterface =
                        enabledEvents.interfaces[theInterface.inherits];
                } else {
                    break; // Top of interface hierarchy
                }
            }
        });
        return events;
    }

    // Set up listeners for all those events which send back the appropriately
    // formatted results when they fire
    function setupPageEventListeners(descriptions) {
        const subscription = Object.keys(descriptions);
        // Set up event handlers
        subscription.forEach(eventName => {
            if (typeof descriptions[eventName] !== "undefined") {
                const listener = event => {
                    // Calculate the id path
                    const path =
                        event.composedPath()
                        .filter(t => t instanceof Element)
                        .map(target => {
                            const pathElement = {
                                tagName: target.tagName.toLowerCase(),
                                attributes: {},
                            };
                            const numAttrs = target.attributes.length;
                            for (let i = numAttrs - 1; i >= 0; i--) {
                                const attribute = target.attributes[i];
                                const name = attribute.name;
                                const value = attribute.value;
                                pathElement.attributes[name] = value;
                            }
                            return pathElement;
                        });

                    // Extract the relevant properties
                    const data = {};
                    Object.entries(descriptions[eventName])
                        .forEach(([property, formatter]) => {
                            data[property] = formatter(event[property]);
                        });
                    sendEvent(eventName, path, data);
                };
                debug("Adding listener:", eventName);
                window.addEventListener(eventName, listener);
                listeners[eventName] = listener;
            } else {
                debug("Invalid event name:", eventName);
            }
        });
    }

    // Fetch the description of the events we wish to support, and add listeners
    // for them to the window object of the page
    const r = new XMLHttpRequest();
    r.onerror = () => debug("Could not fetch list of enabled events!");
    r.onload = () => {
        const enabledEvents = JSON.parse(r.responseText);
        debug(enabledEvents);
        setupPageEventListeners(parseEventDescriptions(enabledEvents));
    };
    r.open("GET", "/.myxine/assets/enabled-events.json");
    r.send();

    // The handlers for events coming from the server:
    function setupServerEventListeners(sse) {
        sse.addEventListener("set", (event) => {
            let data = JSON.parse(event.data);
            document.title = data.title;
            if (data.diff) {
                setBodyTo(data.body);
            } else {
                document.body.innerHTML = data.body;
            }
        });
        sse.addEventListener("refresh", () => window.location.reload());
        sse.addEventListener("evaluate", (event) => evaluateAndRespond(false, event));
        sse.addEventListener("run",      (event) => evaluateAndRespond(true, event));
    }

    // Actually set up SSE...
    let sse = new window.EventSource(window.location.href + "?updates");
    setupServerEventListeners(sse);
    sse.onerror = () => {
        // Set up retry interval to attempt reconnection
        window.setTimeout(() => {
            sse = new window.EventSource(window.location.href + "?updates");
            setupServerEventListeners(sse);
        }, 500); // half a second between retries
    };
});
