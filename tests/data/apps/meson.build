project('test_apps', 'c',
  version : '0.1',
  default_options : ['warning_level=3'])

progs = [
    ['test_project',   'testproject.c'    ],
    ['user_signal',   'user_signal.c'    ],
    ['threads',   'threads.c'    ],
    ['dont_stop', 'dont_stop.c' ],
]

foreach p : progs
    executable(p[0], p[1], install: true)
endforeach
