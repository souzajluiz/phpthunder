<?php

namespace App\Models;

enum Status: string
{
    case Active = 'active';
    case Banned = 'banned';
}
