#!/usr/bin/env python3

import random
import myxine

class Page:
    # The model of the page
    def __init__(self):
        self.x, self.y = 150, 150
        self.hue = random.uniform(0, 360)
        self.radius = 75

    # Draw the page's model as a fragment of HTML
    def draw(self):
        circle_style = f'''
        position: absolute;
        height: {round(self.radius*2)}px;
        width: {round(self.radius*2)}px;
        top: {self.y}px;
        left: {self.x}px;
        transform: translate(-50%, -50%);
        border-radius: 50%;
        border: {round(self.radius/2)}px solid hsl({round(self.hue)}, 80%, 80%);
        background: hsl({round(self.hue+120)}, 80%, 75%)
        '''
        background_style = f'''
        position: absolute;
        overflow: hidden;
        width: 100vw;
        height: 100vh;
        background: hsl({round(self.hue-120)}, 80%, 90%);
        '''
        instructions_style = f'''
        position: absolute;
        bottom: 30px;
        left: 30px;
        font-family: sans-serif;
        font-size: 22pt;
        user-select: none;
        '''
        return f'''
        <div style="{background_style}">
            <div style="{instructions_style}">
                <b>Move the mouse</b> to move the circle<br/>
                <b>Scroll</b> to change the circle's size<br/>
                <b>Ctrl + Scroll</b> to change the color scheme<br/>
                <b>Click</b> to randomize the color scheme<br/>
            </div>
            <div style="{circle_style}"></div>
        </div>
        '''

    # Change the page's model in response to a browser event
    def react(self, event):
        if event.event() == 'mousemove':
            self.x = event.clientX
            self.y = event.clientY
        elif event.event() == 'mousedown':
            self.hue = (self.hue + random.uniform(30, 330)) % 360
        elif event.event() == 'wheel':
            if event.ctrlKey:
                self.hue = (self.hue + event.deltaY * -0.1) % 360
            else:
                self.radius += event.deltaY * -0.2
                self.radius = min(max(self.radius, 12), 1000)

    # The page's event loop
    def run(self, path):
        myxine.update(path, self.draw())          # Draw the page in the browser.
        try:
            for event in myxine.events(path):     # For each browser event,
                self.react(event)                 # update our model of the page,
                myxine.update(path, self.draw())  # then re-draw it in the browser.
        except KeyboardInterrupt:
            pass                                  # Press Ctrl-C to quit.

if __name__ == '__main__':
    Page().run('/')  # Run the page on the root path.
