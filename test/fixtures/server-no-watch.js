'use strict';

// De rooktest gebruikt deze wrapper om de snelle fs.watch-route uit te schakelen.
// server.js moet daarna volledig op de pollingfallback vertrouwen.
const fs = require('node:fs');
fs.watch = () => {
  throw new Error('fs.watch disabled by watcher smoke test');
};

require('../../server.js');
