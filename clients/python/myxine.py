from typing import Optional, Iterator, Dict, List, Any
import requests
from dataclasses import dataclass
from requests import RequestException
import json


# The default port on which myxine operates; can be overridden in the below
# functions if the server is running on another port.
MYXINE_DEFAULT_PORT = 1123


@dataclass
class Target:
    """A Target corresponds to an element in the browser's document. It
    contains a tag name and a mapping from attribute name to attribute value.
    """
    tag: str
    attributes: Dict[str, str]


@dataclass
class Event:
    """An Event from a page has a type, a list of targets, and a set of
    properties keyed by strings, which may be any type.
    """
    type: str
    targets: List[Target]
    properties: Dict[str, Any]

    def __getattr__(self, name) -> Any:
        value = self.properties[name]
        if value is None:
            raise AttributeError
        else:
            return value


def page_url(path: str, port: int = MYXINE_DEFAULT_PORT) -> str:
    """Normalize a port & path to give the localhost url for that location."""
    if len(path) > 0 and path[0] == '/':
        path = path[1:]
    return 'http://localhost:' + str(port) + '/' + path


def events(path: str,
           subscription: Optional[List[str]] = None,
           port: int = MYXINE_DEFAULT_PORT) -> Iterator[Event]:
    """Subscribe to a stream of page events from a myxine server, returning an
    iterator over the events returned by the stream as they become available.
    """
    url = page_url(path, port)
    try:
        params: Dict[str, List[str]]
        if subscription is None:
            url = url + "?events"
            params = {}
        else:
            params = {'events': subscription}
        response = requests.get(url, stream=True, params=params)
        if response.encoding is None:
            response.encoding = 'utf-8'
        for line in response.iter_lines(decode_unicode=True):
            try:
                parsed = json.loads(line)
                yield Event(type=parsed['event'],
                            targets=[Target(tag=j['tagName'],
                                            attributes=j['attributes'])
                                     for j in parsed['targets']],
                            properties=parsed['properties'])
            except json.JSONDecodeError:
                pass
    except RequestException as e:
        msg = "Connection issue with myxine server (is it running?)"
        raise ValueError(msg, e)


def evaluate(path: str, *,
             expression: Optional[str] = None,
             statement: Optional[str] = None,
             timeout: Optional[int] = None,
             port: int = MYXINE_DEFAULT_PORT) -> None:
    """Evaluate the given JavaScript code in the context of the page."""
    bad_args_err = \
        ValueError('Input must be exactly one of a statement or an expression')
    if expression is not None:
        if statement is not None:
            raise bad_args_err
        url = page_url(path, port)
        params = {'evaluate': expression}
        data = expression.encode()
    elif statement is not None:
        if expression is not None:
            raise bad_args_err
        url = page_url(path, port) + '?evaluate'
        params = {}
        data = statement.encode()
    else:
        raise bad_args_err
    if timeout is not None:
        params['timeout'] = str(timeout)
    try:
        r = requests.post(url, data=data, params=params)
        if r.status_code == 200:
            return r.json()
        else:
            raise ValueError(r.text)
    except RequestException as e:
        msg = "Connection issue with myxine server (is it running?)"
        raise ValueError(msg, e)


def update(path: str,
           body: str,
           title: Optional[str] = None,
           port: int = MYXINE_DEFAULT_PORT) -> None:
    """Set the contents of the page at the given path to a provided body and
    title. If body or title is not provided, clears those elements of the page.
    """
    url = page_url(path, port)
    try:
        requests.post(url, data=body.encode(), params={'title': title})
    except RequestException as e:
        msg = "Connection issue with myxine server (is it running?)"
        raise ValueError(msg, e)


def static(path: str,
           body: bytes,
           content_type: str,
           port: int = MYXINE_DEFAULT_PORT) -> None:
    """Set the contents of the page at the given path to the static content
    provided, as a bytestring. You must specify a content type, or else the
    browser won't necessarily know how to display this content.
    """
    url = page_url(path, port) + '?static'
    try:
        requests.post(url, data=body, headers={'Content-Type': content_type})
    except RequestException as e:
        msg = "Connection issue with myxine server (is it running?)"
        raise ValueError(msg, e)