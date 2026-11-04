# The user site's build, as `make docs` and the Docs workflow run it.
FROM python:3.12.15-slim-bookworm
COPY requirements.txt /tmp/requirements.txt
RUN pip install --no-cache-dir --root-user-action=ignore --no-deps -r /tmp/requirements.txt \
    && pip check
ENV HOME=/tmp
