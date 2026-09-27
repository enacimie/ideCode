from animal import Animal


class Gato(Animal):
    def __init__(self, nombre: str, edad: int) -> None:
        self.vidas = 7
        super().__init__(nombre, edad)

    def sonido(self) -> str:
        return "Miau"
